use anyhow::Result;
use sqlx::{sqlite::SqliteConnectOptions, SqlitePool};
use std::str::FromStr;
use tauri::Manager;

pub async fn init(app: &tauri::AppHandle) -> Result<SqlitePool> {
    let data_dir = app
        .path()
        .app_data_dir()
        .expect("failed to get app data dir");
    std::fs::create_dir_all(&data_dir)?;

    let db_path = data_dir.join("meetai.db");
    let db_url = format!("sqlite:{}", db_path.display());

    let opts = SqliteConnectOptions::from_str(&db_url)?
        .create_if_missing(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .foreign_keys(true);

    let pool = SqlitePool::connect_with(opts).await?;

    sqlx::migrate!("./migrations").run(&pool).await?;

    // Create vec0 virtual tables if sqlite-vec is loaded (best-effort; skipped if extension absent)
    let _ = sqlx::query(
        "CREATE VIRTUAL TABLE IF NOT EXISTS vec_kb_chunks USING vec0(embedding float[384])",
    )
    .execute(&pool)
    .await;

    let _ = sqlx::query(
        "CREATE VIRTUAL TABLE IF NOT EXISTS vec_meeting_chunks USING vec0(embedding float[384])",
    )
    .execute(&pool)
    .await;

    Ok(pool)
}
