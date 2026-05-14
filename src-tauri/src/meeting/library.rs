use anyhow::Result;
use serde::Serialize;
use sqlx::SqlitePool;

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct MeetingRow {
    pub id: String,
    pub title: String,
    pub platform: Option<String>,
    pub status: String,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub duration_ms: Option<i64>,
    pub segment_count: i64,
}

#[derive(Debug, Serialize)]
pub struct SourceMeeting {
    pub id: String,
    pub title: String,
    pub started_at: i64,
}

#[derive(Debug, Serialize)]
pub struct ChatResponse {
    pub answer: String,
    pub sources: Vec<SourceMeeting>,
}

// ── Queries ───────────────────────────────────────────────────────────────────

pub async fn list_meetings(pool: &SqlitePool) -> Result<Vec<MeetingRow>> {
    let rows = sqlx::query_as::<
        _,
        (
            String,
            String,
            Option<String>,
            String,
            i64,
            Option<i64>,
            Option<i64>,
            i64,
        ),
    >(
        "SELECT m.id, m.title, m.platform, m.status, m.started_at, m.ended_at, m.duration_ms, \
         COUNT(ts.id) as segment_count \
         FROM meetings m \
         LEFT JOIN transcript_segments ts ON ts.meeting_id = m.id \
         GROUP BY m.id \
         ORDER BY m.started_at DESC \
         LIMIT 200",
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(
            |(id, title, platform, status, started_at, ended_at, duration_ms, segment_count)| {
                MeetingRow {
                    id,
                    title,
                    platform,
                    status,
                    started_at,
                    ended_at,
                    duration_ms,
                    segment_count,
                }
            },
        )
        .collect())
}

/// Search meetings by title or transcript content (LIKE).
pub async fn search_meetings(pool: &SqlitePool, query: &str) -> Result<Vec<MeetingRow>> {
    if query.trim().is_empty() {
        return list_meetings(pool).await;
    }

    let pattern = format!("%{}%", query);
    let rows = sqlx::query_as::<
        _,
        (
            String,
            String,
            Option<String>,
            String,
            i64,
            Option<i64>,
            Option<i64>,
            i64,
        ),
    >(
        "SELECT m.id, m.title, m.platform, m.status, m.started_at, m.ended_at, m.duration_ms, \
         COUNT(ts.id) as segment_count \
         FROM meetings m \
         LEFT JOIN transcript_segments ts ON ts.meeting_id = m.id \
         WHERE m.title LIKE ? \
            OR ts.text LIKE ? \
         GROUP BY m.id \
         ORDER BY m.started_at DESC \
         LIMIT 100",
    )
    .bind(&pattern)
    .bind(&pattern)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(
            |(id, title, platform, status, started_at, ended_at, duration_ms, segment_count)| {
                MeetingRow {
                    id,
                    title,
                    platform,
                    status,
                    started_at,
                    ended_at,
                    duration_ms,
                    segment_count,
                }
            },
        )
        .collect())
}

/// Gather relevant transcript segments for a chat question.
/// Returns (context_string, source_meetings).
pub async fn build_chat_context(
    pool: &SqlitePool,
    question: &str,
) -> Result<(String, Vec<SourceMeeting>)> {
    if question.trim().is_empty() {
        return Ok((String::new(), vec![]));
    }

    let pattern = format!("%{}%", question.trim());

    // Find relevant segments — search title + transcript text
    let rows: Vec<(String, String, String, String, i64, i64)> = sqlx::query_as(
        "SELECT m.id, m.title, ts.source, ts.text, ts.start_ms, m.started_at \
         FROM transcript_segments ts \
         JOIN meetings m ON m.id = ts.meeting_id \
         WHERE m.status != 'recording' \
           AND (ts.text LIKE ? OR m.title LIKE ?) \
         ORDER BY m.started_at DESC, ts.start_ms ASC \
         LIMIT 30",
    )
    .bind(&pattern)
    .bind(&pattern)
    .fetch_all(pool)
    .await?;

    if rows.is_empty() {
        return Ok((String::new(), vec![]));
    }

    // Build context string, de-duplicate source meetings
    let mut context_parts: Vec<String> = Vec::new();
    let mut seen_meetings: Vec<(String, String, i64)> = Vec::new(); // (id, title, started_at)
    let mut current_meeting_id = String::new();

    for (meeting_id, title, source, text, start_ms, started_at) in &rows {
        if meeting_id != &current_meeting_id {
            let date = format_ts(*started_at);
            context_parts.push(format!("\n[Meeting: {title} — {date}]"));
            current_meeting_id = meeting_id.clone();

            if !seen_meetings.iter().any(|(id, _, _)| id == meeting_id) {
                seen_meetings.push((meeting_id.clone(), title.clone(), *started_at));
            }
        }
        let speaker = if source == "you" { "You" } else { "Speaker" };
        let ts_sec = start_ms / 1000;
        let ts_fmt = format!("{}:{:02}", ts_sec / 60, ts_sec % 60);
        context_parts.push(format!("[{ts_fmt}] {speaker}: {text}"));
    }

    let context = context_parts.join("\n");
    let sources = seen_meetings
        .into_iter()
        .map(|(id, title, started_at)| SourceMeeting { id, title, started_at })
        .collect();

    Ok((context, sources))
}

// ── Post-meeting detail ───────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct SummaryData {
    pub overview: String,
    pub decisions: Vec<String>,
    pub topics: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ActionItemData {
    pub id: i64,
    pub text: String,
    pub assignee: Option<String>,
    pub due_date: Option<String>,
    pub done: bool,
}

#[derive(Debug, Serialize)]
pub struct SegmentData {
    pub source: String,
    pub speaker_name: Option<String>,
    pub text: String,
    pub start_ms: i64,
    pub end_ms: i64,
}

#[derive(Debug, Serialize)]
pub struct JobData {
    pub kind: String,
    pub status: String,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct MeetingDetail {
    pub id: String,
    pub title: String,
    pub status: String,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub duration_ms: Option<i64>,
    pub notes: Option<String>,
    pub summary: Option<SummaryData>,
    pub action_items: Vec<ActionItemData>,
    pub segments: Vec<SegmentData>,
    pub jobs: Vec<JobData>,
}

pub async fn get_meeting(pool: &SqlitePool, meeting_id: &str) -> Result<MeetingDetail> {
    // Core meeting row
    let (id, title, status, started_at, ended_at, duration_ms, notes): (
        String, String, String, i64, Option<i64>, Option<i64>, Option<String>,
    ) = sqlx::query_as(
        "SELECT id, title, status, started_at, ended_at, duration_ms, notes \
         FROM meetings WHERE id=?",
    )
    .bind(meeting_id)
    .fetch_one(pool)
    .await?;

    // Summary
    let summary_row: Option<(String, String, String)> = sqlx::query_as(
        "SELECT overview, decisions, topics FROM summaries WHERE meeting_id=?",
    )
    .bind(meeting_id)
    .fetch_optional(pool)
    .await?;

    let summary = summary_row.map(|(overview, decisions_json, topics_json)| {
        let decisions: Vec<String> = serde_json::from_str(&decisions_json).unwrap_or_default();
        let topics: Vec<String> = serde_json::from_str(&topics_json).unwrap_or_default();
        SummaryData { overview, decisions, topics }
    });

    // Action items
    let ai_rows: Vec<(i64, String, Option<String>, Option<String>, i64)> = sqlx::query_as(
        "SELECT id, text, assignee, due_date, done FROM action_items WHERE meeting_id=? ORDER BY id",
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await?;

    let action_items = ai_rows
        .into_iter()
        .map(|(id, text, assignee, due_date, done)| ActionItemData {
            id,
            text,
            assignee,
            due_date,
            done: done != 0,
        })
        .collect();

    // Transcript segments
    let seg_rows: Vec<(String, Option<String>, String, i64, i64)> = sqlx::query_as(
        "SELECT source, speaker_name, text, start_ms, end_ms \
         FROM transcript_segments WHERE meeting_id=? ORDER BY start_ms",
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await?;

    let segments = seg_rows
        .into_iter()
        .map(|(source, speaker_name, text, start_ms, end_ms)| SegmentData {
            source,
            speaker_name,
            text,
            start_ms,
            end_ms,
        })
        .collect();

    // Jobs
    let job_rows: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT kind, status, error FROM jobs WHERE meeting_id=? ORDER BY created_at",
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await?;

    let jobs = job_rows
        .into_iter()
        .map(|(kind, status, error)| JobData { kind, status, error })
        .collect();

    Ok(MeetingDetail {
        id,
        title,
        status,
        started_at,
        ended_at,
        duration_ms,
        notes,
        summary,
        action_items,
        segments,
        jobs,
    })
}

pub async fn toggle_action_item(pool: &SqlitePool, id: i64, done: bool) -> Result<()> {
    sqlx::query("UPDATE action_items SET done=? WHERE id=?")
        .bind(if done { 1i64 } else { 0i64 })
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn save_notes(pool: &SqlitePool, meeting_id: &str, notes: &str) -> Result<()> {
    sqlx::query("UPDATE meetings SET notes=? WHERE id=?")
        .bind(notes)
        .bind(meeting_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn export_markdown(pool: &SqlitePool, meeting_id: &str) -> Result<String> {
    let detail = get_meeting(pool, meeting_id).await?;

    let date_str = format_ts(detail.started_at);
    let duration_str = detail.duration_ms.map(|ms| {
        let s = ms / 1000;
        let m = s / 60;
        let h = m / 60;
        if h > 0 { format!("{h}h {m}m") } else { format!("{m}m {s}s", s = s % 60) }
    }).unwrap_or_else(|| "—".to_string());

    let word_count: usize = detail
        .segments
        .iter()
        .map(|s| s.text.split_whitespace().count())
        .sum();

    let mut md = format!(
        "---\ncreated: {date_str}\ntype: meeting\napp: MeetAI\nduration: {duration_str}\nwords: {word_count}\n---\n\n# {}\n\n",
        detail.title
    );

    if let Some(ref s) = detail.summary {
        md.push_str(&format!("## Overview\n{}\n\n", s.overview));

        if !s.decisions.is_empty() {
            md.push_str("## Key Decisions\n");
            for d in &s.decisions {
                md.push_str(&format!("- {d}\n"));
            }
            md.push('\n');
        }

        if !detail.action_items.is_empty() {
            md.push_str("## Action Items\n");
            for a in &detail.action_items {
                let check = if a.done { "[x]" } else { "[ ]" };
                let assignee = a.assignee.as_deref().map(|n| format!(" ({n})")).unwrap_or_default();
                md.push_str(&format!("- {check} {}{assignee}\n", a.text));
            }
            md.push('\n');
        }

        if !s.topics.is_empty() {
            md.push_str("## Topics\n");
            for t in &s.topics {
                md.push_str(&format!("- {t}\n"));
            }
            md.push('\n');
        }
    }

    if !detail.segments.is_empty() {
        md.push_str("## Transcript\n\n");
        for seg in &detail.segments {
            let speaker = seg.speaker_name.as_deref().unwrap_or(
                if seg.source == "you" { "You" } else { "Speaker" }
            );
            let s = seg.start_ms / 1000;
            md.push_str(&format!(
                "**{speaker}** [{}:{:02}]  \n{}\n\n",
                s / 60, s % 60, seg.text
            ));
        }
    }

    Ok(md)
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn format_ts(ms: i64) -> String {
    // Simple UTC date from Unix ms timestamp
    let secs = ms / 1000;
    let days_since_epoch = secs / 86400;
    // Gregorian calendar approximation
    let year = 1970 + days_since_epoch / 365;
    let day_of_year = days_since_epoch % 365;
    let month = (day_of_year / 30) + 1;
    let day = (day_of_year % 30) + 1;
    format!("{year}-{month:02}-{day:02}")
}
