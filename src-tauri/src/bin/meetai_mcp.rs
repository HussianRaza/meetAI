//! meetai-mcp — stdio MCP server for MeetAI
//!
//! Usage: meetai-mcp [--db /path/to/meetai.db]
//!
//! Implements the Model Context Protocol (MCP) over stdio.
//! Each message is a newline-delimited JSON-RPC 2.0 object.
//! Compatible with Claude Desktop, Cursor, and any MCP v2024-11-05 client.

use anyhow::{Context, Result};
use serde_json::{json, Value};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    Pool, Row, Sqlite,
};
use std::io::{BufRead, BufWriter, Write};
use std::path::PathBuf;

type DB = Pool<Sqlite>;

// ── Entry point ───────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    let db_path = parse_db_path();

    eprintln!("[meetai-mcp] opening DB: {}", db_path.display());

    let pool = open_db(&db_path).await.with_context(|| {
        format!(
            "Cannot open meetai.db at {}. Is MeetAI installed and has at least one session run?",
            db_path.display()
        )
    })?;

    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut out = BufWriter::new(stdout.lock());

    for line in stdin.lock().lines() {
        let line = line?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let req: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("[meetai-mcp] parse error: {e}");
                continue;
            }
        };

        // Notifications have no id — don't respond
        let id = req.get("id").cloned();
        if id.is_none() {
            continue;
        }

        let method = req["method"].as_str().unwrap_or("");
        let response = match dispatch(method, &req, &pool).await {
            Ok(result) => json!({"jsonrpc":"2.0","id":id,"result":result}),
            Err(e) => json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {"code": -32000, "message": e.to_string()}
            }),
        };

        writeln!(out, "{}", response)?;
        out.flush()?;
    }

    Ok(())
}

// ── Protocol dispatch ─────────────────────────────────────────────────────────

async fn dispatch(method: &str, req: &Value, pool: &DB) -> Result<Value> {
    match method {
        "initialize" => Ok(json!({
            "protocolVersion": "2024-11-05",
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "meetai", "version": "0.1.0" }
        })),

        "ping" => Ok(json!({})),

        "tools/list" => Ok(json!({ "tools": tool_definitions() })),

        "resources/list" => Ok(json!({ "resources": [] })),
        "prompts/list" => Ok(json!({ "prompts": [] })),

        "tools/call" => {
            let name = req["params"]["name"].as_str().unwrap_or("");
            let args = &req["params"]["arguments"];
            call_tool(name, args, pool).await
        }

        other => Err(anyhow::anyhow!("method not found: {other}")),
    }
}

// ── Tool definitions ──────────────────────────────────────────────────────────

fn tool_definitions() -> Value {
    json!([
        {
            "name": "search_meetings",
            "description": "Search meeting titles and transcripts by keyword. Returns matching meetings with metadata.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search keyword or phrase" }
                },
                "required": ["query"]
            }
        },
        {
            "name": "get_meeting_transcript",
            "description": "Get the full transcript of a specific meeting, with speaker labels and timestamps.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "meeting_id": { "type": "string", "description": "The UUID of the meeting (from search_meetings or get_recent_meetings)" }
                },
                "required": ["meeting_id"]
            }
        },
        {
            "name": "get_action_items",
            "description": "Get action items. Omit meeting_id to get all pending items across all meetings.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "meeting_id": { "type": "string", "description": "Optional: filter to a specific meeting UUID" }
                }
            }
        },
        {
            "name": "get_recent_meetings",
            "description": "Get the most recent meetings with their AI-generated summaries.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "n": { "type": "integer", "description": "Number of meetings to return (default 5, max 20)" }
                }
            }
        }
    ])
}

// ── Tool implementations ──────────────────────────────────────────────────────

async fn call_tool(name: &str, args: &Value, pool: &DB) -> Result<Value> {
    let text = match name {
        "search_meetings" => search_meetings(args, pool).await?,
        "get_meeting_transcript" => get_meeting_transcript(args, pool).await?,
        "get_action_items" => get_action_items(args, pool).await?,
        "get_recent_meetings" => get_recent_meetings(args, pool).await?,
        other => return Err(anyhow::anyhow!("unknown tool: {other}")),
    };

    Ok(json!({ "content": [{ "type": "text", "text": text }] }))
}

async fn search_meetings(args: &Value, pool: &DB) -> Result<String> {
    let query = args["query"].as_str().unwrap_or("").trim();
    if query.is_empty() {
        return Ok("Please provide a non-empty search query.".into());
    }
    let pat = format!("%{query}%");

    let rows = sqlx::query(
        "SELECT m.id, m.title, m.status, m.started_at, m.duration_ms
         FROM meetings m
         WHERE m.title LIKE ?
            OR m.id IN (
               SELECT DISTINCT meeting_id FROM transcript_segments WHERE text LIKE ?
            )
         ORDER BY m.started_at DESC
         LIMIT 10",
    )
    .bind(&pat)
    .bind(&pat)
    .fetch_all(pool)
    .await?;

    if rows.is_empty() {
        return Ok(format!("No meetings found matching '{query}'."));
    }

    let mut out = format!("Found {} meeting(s) matching '{query}':\n\n", rows.len());
    for row in &rows {
        let id: String = row.try_get("id")?;
        let title: String = row.try_get("title")?;
        let status: String = row.try_get("status")?;
        let started_at: i64 = row.try_get("started_at")?;
        let duration_ms: Option<i64> = row.try_get("duration_ms")?;

        out.push_str(&format!(
            "- **{title}**\n  ID: {id}\n  Date: {}\n  Duration: {}  Status: {status}\n\n",
            fmt_ts(started_at),
            duration_ms.map(fmt_dur).unwrap_or_else(|| "—".into()),
        ));
    }
    Ok(out)
}

async fn get_meeting_transcript(args: &Value, pool: &DB) -> Result<String> {
    let mid = args["meeting_id"].as_str().unwrap_or("").trim();
    if mid.is_empty() {
        return Ok("meeting_id is required.".into());
    }

    let meeting = sqlx::query("SELECT title, started_at FROM meetings WHERE id = ?")
        .bind(mid)
        .fetch_optional(pool)
        .await?;

    let Some(meeting) = meeting else {
        return Ok(format!("Meeting '{mid}' not found."));
    };

    let title: String = meeting.try_get("title")?;
    let started_at: i64 = meeting.try_get("started_at")?;

    let segs = sqlx::query(
        "SELECT source, speaker_name, text, start_ms
         FROM transcript_segments
         WHERE meeting_id = ? AND is_final = 1
         ORDER BY start_ms",
    )
    .bind(mid)
    .fetch_all(pool)
    .await?;

    let mut out = format!("# {title}\nDate: {}\n\n", fmt_ts(started_at));

    for seg in &segs {
        let source: String = seg.try_get("source")?;
        let speaker_name: Option<String> = seg.try_get("speaker_name")?;
        let text: String = seg.try_get("text")?;
        let start_ms: i64 = seg.try_get("start_ms")?;

        let speaker = speaker_name.unwrap_or_else(|| {
            if source == "you" {
                "You".into()
            } else {
                "Speaker".into()
            }
        });
        out.push_str(&format!("[{}] {speaker}: {text}\n", fmt_ms(start_ms)));
    }

    if segs.is_empty() {
        out.push_str("No transcript segments recorded.");
    }
    Ok(out)
}

async fn get_action_items(args: &Value, pool: &DB) -> Result<String> {
    let mid = args.get("meeting_id").and_then(|v| v.as_str());

    let rows = if let Some(mid) = mid {
        sqlx::query(
            "SELECT ai.text, ai.assignee, ai.done, m.title
             FROM action_items ai JOIN meetings m ON m.id = ai.meeting_id
             WHERE ai.meeting_id = ?
             ORDER BY ai.id",
        )
        .bind(mid)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query(
            "SELECT ai.text, ai.assignee, ai.done, m.title
             FROM action_items ai JOIN meetings m ON m.id = ai.meeting_id
             WHERE ai.done = 0
             ORDER BY ai.id DESC
             LIMIT 50",
        )
        .fetch_all(pool)
        .await?
    };

    let header = if mid.is_some() {
        "Action items:\n\n"
    } else {
        "Pending action items across all meetings:\n\n"
    };
    let mut out = header.to_string();

    if rows.is_empty() {
        out.push_str("No action items found.");
        return Ok(out);
    }

    for row in &rows {
        let text: String = row.try_get("text")?;
        let assignee: Option<String> = row.try_get("assignee")?;
        let done: bool = row.try_get("done")?;
        let meeting_title: String = row.try_get("title")?;

        let check = if done { "[x]" } else { "[ ]" };
        let who = assignee.map(|a| format!(" → {a}")).unwrap_or_default();
        out.push_str(&format!("{check} {text}{who}\n    _(from: {meeting_title})_\n"));
    }
    Ok(out)
}

async fn get_recent_meetings(args: &Value, pool: &DB) -> Result<String> {
    let n = args
        .get("n")
        .and_then(|v| v.as_i64())
        .unwrap_or(5)
        .clamp(1, 20) as i32;

    let rows = sqlx::query(
        "SELECT m.id, m.title, m.status, m.started_at, m.duration_ms, s.overview
         FROM meetings m
         LEFT JOIN summaries s ON s.meeting_id = m.id
         ORDER BY m.started_at DESC
         LIMIT ?",
    )
    .bind(n)
    .fetch_all(pool)
    .await?;

    if rows.is_empty() {
        return Ok("No meetings recorded yet.".into());
    }

    let mut out = format!("Last {} meeting(s):\n\n", rows.len());
    for row in &rows {
        let id: String = row.try_get("id")?;
        let title: String = row.try_get("title")?;
        let status: String = row.try_get("status")?;
        let started_at: i64 = row.try_get("started_at")?;
        let duration_ms: Option<i64> = row.try_get("duration_ms")?;
        let overview: Option<String> = row.try_get("overview")?;

        let dur = duration_ms.map(fmt_dur).unwrap_or_else(|| "—".into());
        out.push_str(&format!(
            "## {title}\nID: `{id}`\nDate: {}  Duration: {dur}  Status: {status}\n",
            fmt_ts(started_at)
        ));
        if let Some(ov) = overview {
            out.push_str(&format!("{ov}\n"));
        }
        out.push('\n');
    }
    Ok(out)
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn parse_db_path() -> PathBuf {
    let args: Vec<String> = std::env::args().collect();
    if let Some(pos) = args.iter().position(|a| a == "--db") {
        if let Some(p) = args.get(pos + 1) {
            return PathBuf::from(p);
        }
    }
    // Fallback: compute from XDG standard (Linux/macOS)
    let xdg = std::env::var("XDG_DATA_HOME").unwrap_or_else(|_| {
        format!("{}/.local/share", std::env::var("HOME").unwrap_or_default())
    });
    PathBuf::from(xdg).join("com.hussain.meetai").join("meetai.db")
}

async fn open_db(path: &PathBuf) -> Result<DB> {
    let opts = SqliteConnectOptions::new()
        .filename(path)
        .read_only(true)
        .create_if_missing(false);
    SqlitePoolOptions::new()
        .max_connections(2)
        .connect_with(opts)
        .await
        .map_err(Into::into)
}

/// Format Unix-millisecond timestamp as "YYYY-MM-DD HH:MM UTC"
fn fmt_ts(ms: i64) -> String {
    let secs = ms / 1000;
    let (y, mo, d) = days_to_ymd(secs / 86400);
    let rem = secs % 86400;
    let h = rem / 3600;
    let mi = (rem % 3600) / 60;
    format!("{y}-{mo:02}-{d:02} {h:02}:{mi:02} UTC")
}

/// Format millisecond duration as "Xm Ys"
fn fmt_dur(ms: i64) -> String {
    let s = ms / 1000;
    let m = s / 60;
    let h = m / 60;
    if h > 0 {
        format!("{h}h {}m", m % 60)
    } else {
        format!("{m}m {}s", s % 60)
    }
}

/// Format millisecond offset as "M:SS"
fn fmt_ms(ms: i64) -> String {
    let s = ms / 1000;
    format!("{}:{:02}", s / 60, s % 60)
}

/// Convert days-since-Unix-epoch to (year, month, day) — Gregorian.
/// Algorithm: http://howardhinnant.github.io/date_algorithms.html
fn days_to_ymd(days: i64) -> (i64, u8, u8) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m as u8, d as u8)
}
