use anyhow::Result;
use serde::Serialize;
use sqlx::{Row, SqlitePool};

use super::embed::{self, bytes_to_embedding, EmbedModel};

#[derive(Debug, Serialize, Clone)]
pub struct SearchResult {
    pub chunk_id: i64,
    pub file_path: String,
    pub breadcrumb: String,
    pub snippet: String,
    pub score: f32,
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na == 0.0 || nb == 0.0 {
        0.0
    } else {
        dot / (na * nb)
    }
}

pub async fn search(
    pool: &SqlitePool,
    model: &EmbedModel,
    query: &str,
    top_k: usize,
) -> Result<Vec<SearchResult>> {
    // Embed the query
    let q_vecs = embed::embed(model, vec![query.to_string()]).await?;
    let q_emb = &q_vecs[0];

    // Load all chunks with embeddings
    let rows = sqlx::query(
        r#"
        SELECT c.id, f.path, c.breadcrumb, c.text, c.embedding
        FROM kb_chunks c
        JOIN kb_files f ON f.id = c.file_id
        WHERE c.embedding IS NOT NULL
        "#,
    )
    .fetch_all(pool)
    .await?;

    let mut scored: Vec<SearchResult> = rows
        .iter()
        .filter_map(|row| {
            let blob: Vec<u8> = row.get("embedding");
            if blob.is_empty() {
                return None;
            }
            let chunk_emb = bytes_to_embedding(&blob);
            let score = cosine(q_emb, &chunk_emb);
            if score < 0.25 {
                return None;
            }
            Some(SearchResult {
                chunk_id: row.get("id"),
                file_path: row.get("path"),
                breadcrumb: row.get::<Option<String>, _>("breadcrumb").unwrap_or_default(),
                snippet: {
                    let text: String = row.get("text");
                    let words: Vec<&str> = text.split_whitespace().take(40).collect();
                    words.join(" ")
                },
                score,
            })
        })
        .collect();

    scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
    scored.truncate(top_k);
    Ok(scored)
}
