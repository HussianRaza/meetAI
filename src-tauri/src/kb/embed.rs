use anyhow::Result;
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use std::path::Path;
use std::sync::{Arc, Mutex};

pub type EmbedModel = Arc<Mutex<Option<TextEmbedding>>>;

pub fn new_handle() -> EmbedModel {
    Arc::new(Mutex::new(None))
}

/// Initialise the embedding model (downloads ~30 MB on first run).
/// Runs in spawn_blocking — safe to call from async context.
pub async fn ensure_init(model: &EmbedModel, cache_dir: &Path) -> Result<()> {
    {
        let lock = model.lock().unwrap();
        if lock.is_some() {
            return Ok(()); // Already initialised
        }
    }
    let dir = cache_dir.to_path_buf();
    let model_clone = Arc::clone(model);
    tokio::task::spawn_blocking(move || -> Result<()> {
        let te = TextEmbedding::try_new(
            InitOptions::new(EmbeddingModel::BGESmallENV15).with_cache_dir(dir),
        )?;
        let mut lock = model_clone.lock().unwrap();
        *lock = Some(te);
        Ok(())
    })
    .await??;
    Ok(())
}

/// Embed a batch of texts, returning 384-dim f32 vectors.
pub async fn embed(model: &EmbedModel, texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
    let model_clone = Arc::clone(model);
    tokio::task::spawn_blocking(move || {
        let lock = model_clone.lock().unwrap();
        match lock.as_ref() {
            Some(m) => m.embed(texts, Some(32)).map_err(anyhow::Error::from),
            None => Err(anyhow::anyhow!("Embed model not initialized")),
        }
    })
    .await?
}

pub fn embedding_to_bytes(emb: &[f32]) -> Vec<u8> {
    emb.iter().flat_map(|f| f.to_le_bytes()).collect()
}

pub fn bytes_to_embedding(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}
