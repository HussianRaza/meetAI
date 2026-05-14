use anyhow::Result;
use serde::Serialize;
use sqlx::SqlitePool;
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tauri::Emitter;

use crate::{kb::embed::EmbedModel, llm};

// ── Public types ──────────────────────────────────────────────────────────────

pub struct NudgeSettings {
    pub enabled: bool,
    pub ai_suggestions: bool,
    pub interval_secs: u64,
    pub threshold: f32,
    pub groq_key: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct NudgeCard {
    pub id: String,
    pub file_path: String,
    pub breadcrumb: String,
    pub snippet: String,
    pub score: f32,
    pub suggestion: Option<String>,
}

// ── Entry point ───────────────────────────────────────────────────────────────

/// Start the nudge engine task. Returns None if nudge is disabled.
pub fn start(
    meeting_id: String,
    settings: NudgeSettings,
    pool: SqlitePool,
    embed_model: EmbedModel,
    stop: Arc<AtomicBool>,
    app: tauri::AppHandle,
) -> Option<tokio::task::JoinHandle<()>> {
    if !settings.enabled {
        return None;
    }

    let handle = tokio::spawn(async move {
        run_loop(meeting_id, settings, pool, embed_model, stop, app).await;
    });

    Some(handle)
}

// ── Engine loop ───────────────────────────────────────────────────────────────

async fn run_loop(
    meeting_id: String,
    settings: NudgeSettings,
    pool: SqlitePool,
    embed_model: EmbedModel,
    stop: Arc<AtomicBool>,
    app: tauri::AppHandle,
) {
    let client = reqwest::Client::new();
    // Ring buffer of (id, snippet) for Jaccard dedup
    let mut recent: VecDeque<(String, String)> = VecDeque::new();
    let mut card_counter: u32 = 0;

    loop {
        // Interruptible sleep: 100 ms ticks
        for _ in 0..(settings.interval_secs * 10) {
            if stop.load(Ordering::Relaxed) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        if stop.load(Ordering::Relaxed) {
            return;
        }

        // Get last 40 words from transcript
        let query = match last_40_words(&pool, &meeting_id).await {
            Ok(q) if !q.is_empty() => q,
            _ => continue,
        };

        // KB search
        let results =
            match crate::kb::search::search(&pool, &embed_model, &query, 3).await {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("[nudge] search error: {e}");
                    continue;
                }
            };

        // Find first result above threshold that's not a duplicate
        let candidate = results.into_iter().find(|r| {
            if r.score < settings.threshold {
                return false;
            }
            !recent
                .iter()
                .any(|(_, s)| jaccard(&r.snippet, s) > 0.7)
        });

        let result = match candidate {
            Some(r) => r,
            None => continue,
        };

        // AI suggestion (optional, with timeout)
        let suggestion = if settings.ai_suggestions && !settings.groq_key.is_empty() {
            let rolling_ctx = rolling_context(&pool, &meeting_id).await.unwrap_or_default();
            let msgs = build_messages(&rolling_ctx, &result.breadcrumb, &result.snippet);
            match tokio::time::timeout(
                Duration::from_secs(12),
                llm::chat(&client, &settings.groq_key, llm::MODEL_LIVE, msgs, 120),
            )
            .await
            {
                Ok(Ok(text)) => Some(text),
                Ok(Err(e)) => {
                    eprintln!("[nudge] groq error: {e}");
                    None
                }
                Err(_) => {
                    eprintln!("[nudge] groq timeout");
                    None
                }
            }
        } else {
            None
        };

        card_counter += 1;
        let id = format!("nudge-{card_counter}");

        let card = NudgeCard {
            id: id.clone(),
            file_path: result.file_path.clone(),
            breadcrumb: result.breadcrumb.clone(),
            snippet: result.snippet.clone(),
            score: result.score,
            suggestion,
        };

        let _ = app.emit("nudge-update", &card);

        // Add to ring (max 3 for dedup window)
        recent.push_back((id, result.snippet));
        if recent.len() > 3 {
            recent.pop_front();
        }
    }
}

// ── DB helpers ────────────────────────────────────────────────────────────────

async fn last_40_words(pool: &SqlitePool, meeting_id: &str) -> Result<String> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT text FROM transcript_segments \
         WHERE meeting_id=? ORDER BY created_at DESC LIMIT 10",
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await?;

    // Rows are newest-first; reverse to get chronological order, then join
    let all_text: String = rows
        .iter()
        .rev()
        .map(|(t,)| t.as_str())
        .collect::<Vec<_>>()
        .join(" ");

    let words: Vec<&str> = all_text.split_whitespace().collect();
    let start = words.len().saturating_sub(40);
    Ok(words[start..].join(" "))
}

/// Last 120 s of transcript, formatted for the LLM prompt.
async fn rolling_context(pool: &SqlitePool, meeting_id: &str) -> Result<String> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT source, text FROM transcript_segments \
         WHERE meeting_id=? AND created_at > (unixepoch()*1000 - 120000) \
         ORDER BY created_at ASC LIMIT 50",
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await?;

    let ctx = rows
        .iter()
        .map(|(src, txt)| {
            let label = if src == "you" { "You" } else { "Speaker" };
            format!("{label}: {txt}")
        })
        .collect::<Vec<_>>()
        .join("\n");

    Ok(ctx)
}

// ── LLM prompt ───────────────────────────────────────────────────────────────

fn build_messages(
    context: &str,
    breadcrumb: &str,
    snippet: &str,
) -> Vec<serde_json::Value> {
    let system = "You are a real-time meeting assistant. Based on the meeting transcript \
                  and a matched knowledge-base excerpt, produce one concise talking point \
                  (1–2 sentences) the speaker could raise right now. Be direct and specific. \
                  Do not include preamble.";

    let user = format!(
        "Meeting (last 2 min):\n{context}\n\n\
         Knowledge base ({breadcrumb}):\n{snippet}\n\n\
         Talking point:"
    );

    vec![
        serde_json::json!({"role": "system", "content": system}),
        serde_json::json!({"role": "user", "content": user}),
    ]
}

// ── Jaccard similarity ────────────────────────────────────────────────────────

fn jaccard(a: &str, b: &str) -> f32 {
    use std::collections::HashSet;
    let a_set: HashSet<&str> = a.split_whitespace().collect();
    let b_set: HashSet<&str> = b.split_whitespace().collect();
    if a_set.is_empty() && b_set.is_empty() {
        return 1.0;
    }
    let inter = a_set.intersection(&b_set).count();
    let union_count = a_set.len() + b_set.len() - inter;
    if union_count == 0 {
        return 0.0;
    }
    inter as f32 / union_count as f32
}
