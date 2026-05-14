use anyhow::Result;
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::{Row, SqlitePool};
use std::path::Path;
use tauri::Emitter;
use walkdir::WalkDir;

use super::{
    chunker::chunk_markdown,
    embed::{self, embedding_to_bytes, EmbedModel},
};

#[derive(Serialize, Clone)]
pub struct IndexProgress {
    pub current: usize,
    pub total: usize,
    pub file: String,
    pub done: bool,
}

static EXTENSIONS: &[&str] = &["md", "txt", "markdown"];

fn is_indexable(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| EXTENSIONS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

fn sha256_of(content: &str) -> String {
    hex::encode(Sha256::digest(content.as_bytes()))
}

/// Full index of a folder. Emits `kb-index-progress` events via `app`.
pub async fn index_folder(
    folder: &str,
    pool: &SqlitePool,
    model: &EmbedModel,
    app: &tauri::AppHandle,
) -> Result<(usize, usize)> {
    let files: Vec<_> = WalkDir::new(folder)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file() && is_indexable(e.path()))
        .collect();

    let total = files.len();
    let mut indexed_files = 0usize;
    let mut total_chunks = 0usize;

    for (i, entry) in files.iter().enumerate() {
        let path = entry.path();
        let path_str = path.to_string_lossy().to_string();

        let _ = app.emit(
            "kb-index-progress",
            IndexProgress {
                current: i,
                total,
                file: path.file_name().unwrap_or_default().to_string_lossy().into(),
                done: false,
            },
        );

        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let hash = sha256_of(&content);

        // Check if file changed
        let existing: Option<String> = sqlx::query(
            "SELECT sha256 FROM kb_files WHERE path = ?",
        )
        .bind(&path_str)
        .fetch_optional(pool)
        .await?
        .map(|r| r.get("sha256"));

        if existing.as_deref() == Some(&hash) {
            continue; // Unchanged — skip
        }

        total_chunks += index_file_content(&path_str, &content, &hash, pool, model).await?;
        indexed_files += 1;
    }

    let _ = app.emit(
        "kb-index-progress",
        IndexProgress {
            current: total,
            total,
            file: String::new(),
            done: true,
        },
    );

    Ok((indexed_files, total_chunks))
}

/// Reindex a single file (called from watcher on change).
pub async fn index_single_file(
    path: &Path,
    pool: &SqlitePool,
    model: &EmbedModel,
) -> Result<usize> {
    if !is_indexable(path) {
        return Ok(0);
    }
    let path_str = path.to_string_lossy().to_string();
    let content = std::fs::read_to_string(path)?;
    let hash = sha256_of(&content);

    let existing: Option<String> = sqlx::query("SELECT sha256 FROM kb_files WHERE path = ?")
        .bind(&path_str)
        .fetch_optional(pool)
        .await?
        .map(|r| r.get("sha256"));

    if existing.as_deref() == Some(&hash) {
        return Ok(0);
    }

    index_file_content(&path_str, &content, &hash, pool, model).await
}

async fn index_file_content(
    path_str: &str,
    content: &str,
    hash: &str,
    pool: &SqlitePool,
    model: &EmbedModel,
) -> Result<usize> {
    // Upsert file record
    let file_id: i64 = sqlx::query(
        "INSERT INTO kb_files(path, sha256) VALUES(?, ?)
         ON CONFLICT(path) DO UPDATE SET sha256=excluded.sha256
         RETURNING id",
    )
    .bind(path_str)
    .bind(hash)
    .fetch_one(pool)
    .await?
    .get("id");

    // Delete old chunks for this file
    sqlx::query("DELETE FROM kb_chunks WHERE file_id = ?")
        .bind(file_id)
        .execute(pool)
        .await?;

    let chunks = chunk_markdown(content);
    if chunks.is_empty() {
        return Ok(0);
    }

    // Embed all chunk texts in one batch
    let texts: Vec<String> = chunks.iter().map(|c| c.text.clone()).collect();
    let embeddings = embed::embed(model, texts).await?;

    // Insert each chunk
    for (chunk, emb) in chunks.iter().zip(embeddings.iter()) {
        let blob = embedding_to_bytes(emb);
        sqlx::query(
            "INSERT INTO kb_chunks(file_id, chunk_index, text, breadcrumb, embedding)
             VALUES(?, ?, ?, ?, ?)",
        )
        .bind(file_id)
        .bind(chunk.chunk_index as i64)
        .bind(&chunk.text)
        .bind(&chunk.breadcrumb)
        .bind(&blob)
        .execute(pool)
        .await?;
    }

    Ok(chunks.len())
}
