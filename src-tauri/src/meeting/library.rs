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
