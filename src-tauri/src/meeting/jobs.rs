/// Background job queue — runs Embed and Summarize after each meeting.
use anyhow::{anyhow, Result};
use std::path::PathBuf;
use tauri::Emitter;
use tokio::sync::mpsc;

use crate::kb::embed::EmbedModel;
use crate::llm;
use sqlx::SqlitePool;

// ── Public types ──────────────────────────────────────────────────────────────

pub enum JobKind {
    Embed,
    Summarize,
}

impl JobKind {
    fn as_str(&self) -> &'static str {
        match self {
            JobKind::Embed => "Embed",
            JobKind::Summarize => "Summarize",
        }
    }
}

pub struct JobRequest {
    pub meeting_id: String,
    pub kind: JobKind,
}

// ── Queue runner ──────────────────────────────────────────────────────────────

pub async fn run_queue(
    mut rx: mpsc::Receiver<JobRequest>,
    pool: SqlitePool,
    embed_model: EmbedModel,
    data_dir: PathBuf,
    app: tauri::AppHandle,
) {
    while let Some(req) = rx.recv().await {
        let kind_str = req.kind.as_str();
        let job_id = insert_job(&pool, &req.meeting_id, kind_str).await;
        mark_running(&pool, job_id).await;
        emit(&app, &req.meeting_id, kind_str, "running", None);

        let result = match req.kind {
            JobKind::Embed => {
                embed_job(&pool, &embed_model, &data_dir, &req.meeting_id).await
            }
            JobKind::Summarize => summarize_job(&pool, &req.meeting_id, &app).await,
        };

        match result {
            Ok(_) => {
                mark_done(&pool, job_id).await;
                emit(&app, &req.meeting_id, kind_str, "done", None);
            }
            Err(e) => {
                let err = e.to_string();
                eprintln!("[jobs] {kind_str} failed for {}: {err}", req.meeting_id);
                mark_error(&pool, job_id, &err).await;
                emit(&app, &req.meeting_id, kind_str, "error", Some(&err));
            }
        }

        // After Summarize, check if all jobs done → mark meeting 'done'
        if matches!(req.kind, JobKind::Summarize) {
            let _ = sqlx::query(
                "UPDATE meetings SET status='done' \
                 WHERE id=? AND status='processing'",
            )
            .bind(&req.meeting_id)
            .execute(&pool)
            .await;
            emit(&app, &req.meeting_id, "Meeting", "done", None);
        }
    }
}

// ── Embed job ─────────────────────────────────────────────────────────────────

async fn embed_job(
    pool: &SqlitePool,
    model: &EmbedModel,
    data_dir: &PathBuf,
    meeting_id: &str,
) -> Result<()> {
    let rows: Vec<(String, String, i64, i64)> = sqlx::query_as(
        "SELECT source, text, start_ms, end_ms \
         FROM transcript_segments WHERE meeting_id=? ORDER BY start_ms",
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await?;

    if rows.is_empty() {
        return Ok(());
    }

    // Chunk by ~200-word windows
    let mut chunks: Vec<(String, i64, i64)> = Vec::new();
    let mut buf: Vec<String> = Vec::new();
    let mut chunk_start = rows[0].2;
    let mut chunk_end = 0i64;

    for (source, text, start_ms, end_ms) in &rows {
        let speaker = if source == "you" { "You" } else { "Speaker" };
        buf.extend(
            format!("{speaker}: {text}")
                .split_whitespace()
                .map(str::to_string),
        );
        chunk_end = *end_ms;

        if buf.len() >= 200 {
            chunks.push((buf.join(" "), chunk_start, chunk_end));
            buf.clear();
            chunk_start = *start_ms;
        }
    }
    if !buf.is_empty() {
        chunks.push((buf.join(" "), chunk_start, chunk_end));
    }

    // Init embed model
    let models_dir = data_dir.join("models");
    crate::kb::embed::ensure_init(model, &models_dir).await?;

    // Replace existing chunks
    sqlx::query("DELETE FROM meeting_chunks WHERE meeting_id=?")
        .bind(meeting_id)
        .execute(pool)
        .await?;

    for (i, (text, start_ms, end_ms)) in chunks.iter().enumerate() {
        let embeddings = crate::kb::embed::embed(model, vec![text.clone()]).await?;
        let emb_bytes = crate::kb::embed::embedding_to_bytes(&embeddings[0]);
        sqlx::query(
            "INSERT INTO meeting_chunks \
             (meeting_id, chunk_index, text, start_ms, end_ms, embedding) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(meeting_id)
        .bind(i as i64)
        .bind(text)
        .bind(start_ms)
        .bind(end_ms)
        .bind(&emb_bytes)
        .execute(pool)
        .await?;
    }

    eprintln!("[jobs] embedded {} chunks for {meeting_id}", chunks.len());
    Ok(())
}

// ── Summarize job ─────────────────────────────────────────────────────────────

const SUMMARIZE_SYSTEM: &str = "\
You are a professional meeting summarizer. \
Analyze the transcript and respond with ONLY a JSON object — no markdown fences, no explanation.\
";

const SUMMARIZE_TEMPLATE: &str = r#"Summarize this meeting transcript as JSON:
{
  "overview": "<2-3 sentence summary>",
  "decisions": ["<decision>", ...],
  "action_items": [{"text": "<task>", "assignee": null, "due": null}, ...],
  "topics": ["<topic>", ...]
}

Transcript:
"#;

async fn summarize_job(
    pool: &SqlitePool,
    meeting_id: &str,
    app: &tauri::AppHandle,
) -> Result<()> {
    let cfg = crate::settings::get(pool).await?;
    if cfg.groq_key.is_empty() {
        return Err(anyhow!("Groq API key not configured"));
    }

    // Build transcript text
    let rows: Vec<(String, String, i64)> = sqlx::query_as(
        "SELECT source, text, start_ms FROM transcript_segments \
         WHERE meeting_id=? ORDER BY start_ms",
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await?;

    if rows.is_empty() {
        return Err(anyhow!("No transcript segments to summarize"));
    }

    let transcript: String = rows
        .iter()
        .map(|(src, txt, ms)| {
            let speaker = if src == "you" { "You" } else { "Speaker" };
            let s = ms / 1000;
            format!("{speaker} [{}:{:02}]: {txt}", s / 60, s % 60)
        })
        .collect::<Vec<_>>()
        .join("\n");

    let user_content = format!("{SUMMARIZE_TEMPLATE}{transcript}");
    let messages = vec![
        serde_json::json!({"role": "system", "content": SUMMARIZE_SYSTEM}),
        serde_json::json!({"role": "user",   "content": user_content}),
    ];

    let client = reqwest::Client::new();
    let raw = tokio::time::timeout(
        std::time::Duration::from_secs(60),
        llm::chat(
            &client,
            &cfg.groq_key,
            "llama-3.3-70b-versatile",
            messages,
            1000,
        ),
    )
    .await
    .map_err(|_| anyhow!("Summarize timed out"))??;

    // Strip optional markdown fences
    let json_str = raw
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    let json: serde_json::Value =
        serde_json::from_str(json_str).map_err(|e| anyhow!("JSON parse: {e}\n{json_str}"))?;

    let overview = json["overview"].as_str().unwrap_or("").to_string();
    let decisions = json["decisions"].clone();
    let topics = json["topics"].clone();

    // Upsert summaries row
    sqlx::query(
        "INSERT INTO summaries (meeting_id, overview, decisions, topics) \
         VALUES (?, ?, ?, ?) \
         ON CONFLICT(meeting_id) DO UPDATE SET \
           overview=excluded.overview, \
           decisions=excluded.decisions, \
           topics=excluded.topics",
    )
    .bind(meeting_id)
    .bind(&overview)
    .bind(decisions.to_string())
    .bind(topics.to_string())
    .execute(pool)
    .await?;

    // Replace action items
    sqlx::query("DELETE FROM action_items WHERE meeting_id=?")
        .bind(meeting_id)
        .execute(pool)
        .await?;

    if let Some(items) = json["action_items"].as_array() {
        for item in items {
            let text = item["text"].as_str().unwrap_or("").to_string();
            if text.is_empty() {
                continue;
            }
            let assignee = item["assignee"].as_str().map(str::to_string);
            let due = item["due"].as_str().map(str::to_string);
            sqlx::query(
                "INSERT INTO action_items (meeting_id, text, assignee, due_date) \
                 VALUES (?, ?, ?, ?)",
            )
            .bind(meeting_id)
            .bind(&text)
            .bind(assignee)
            .bind(due)
            .execute(pool)
            .await?;
        }
    }

    eprintln!("[jobs] summarized {meeting_id}");

    // Obsidian vault auto-export
    if !cfg.obsidian_vault.is_empty() {
        if let Ok(md) = crate::meeting::library::export_markdown(pool, meeting_id).await {
            let vault = std::path::PathBuf::from(&cfg.obsidian_vault);
            let meetings_dir = vault.join("Meetings");
            let _ = tokio::fs::create_dir_all(&meetings_dir).await;
            let (title,): (String,) = sqlx::query_as(
                "SELECT title FROM meetings WHERE id=?",
            )
            .bind(meeting_id)
            .fetch_one(pool)
            .await
            .unwrap_or(("untitled".to_string(),));
            let safe_title: String = title
                .chars()
                .map(|c| if c.is_alphanumeric() || c == ' ' { c } else { '-' })
                .collect();
            let filename = format!("{safe_title}.md");
            let _ = tokio::fs::write(meetings_dir.join(filename), md).await;
            let _ = app.emit(
                "obsidian-export-done",
                serde_json::json!({"meeting_id": meeting_id}),
            );
        }
    }

    Ok(())
}

// ── DB helpers ────────────────────────────────────────────────────────────────

async fn insert_job(pool: &SqlitePool, meeting_id: &str, kind: &str) -> i64 {
    let result = sqlx::query(
        "INSERT INTO jobs (meeting_id, kind, status) VALUES (?, ?, 'pending')",
    )
    .bind(meeting_id)
    .bind(kind)
    .execute(pool)
    .await;
    result.map(|r| r.last_insert_rowid()).unwrap_or(0)
}

async fn mark_running(pool: &SqlitePool, id: i64) {
    let _ = sqlx::query(
        "UPDATE jobs SET status='running', started_at=(unixepoch()*1000) WHERE id=?",
    )
    .bind(id)
    .execute(pool)
    .await;
}

async fn mark_done(pool: &SqlitePool, id: i64) {
    let _ = sqlx::query(
        "UPDATE jobs SET status='done', finished_at=(unixepoch()*1000) WHERE id=?",
    )
    .bind(id)
    .execute(pool)
    .await;
}

async fn mark_error(pool: &SqlitePool, id: i64, error: &str) {
    let _ = sqlx::query(
        "UPDATE jobs SET status='error', error=?, finished_at=(unixepoch()*1000) WHERE id=?",
    )
    .bind(error)
    .bind(id)
    .execute(pool)
    .await;
}

fn emit(app: &tauri::AppHandle, meeting_id: &str, kind: &str, status: &str, error: Option<&str>) {
    let _ = app.emit(
        "job-progress",
        serde_json::json!({
            "meeting_id": meeting_id,
            "kind": kind,
            "status": status,
            "error": error,
        }),
    );
}
