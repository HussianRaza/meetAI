use anyhow::{anyhow, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::Emitter;
use tokio::io::AsyncWriteExt;

pub type WhisperModel = Arc<tokio::sync::Mutex<Option<Arc<whisper_rs::WhisperContext>>>>;

pub fn new_handle() -> WhisperModel {
    Arc::new(tokio::sync::Mutex::new(None))
}

/// Map a settings model name to the ggml filename.
pub fn model_path_for(model_name: &str, models_dir: &Path) -> PathBuf {
    let filename = match model_name {
        "whisper-base" | "base" => "ggml-base.en.bin",
        _ => "ggml-tiny.en.bin", // default to tiny
    };
    models_dir.join(filename)
}

#[derive(Debug, Serialize)]
pub struct WhisperStatus {
    pub ready: bool,
    pub model_name: String,
    pub model_path: String,
}

/// Load the model into memory if not already loaded.
pub async fn ensure_loaded(model: &WhisperModel, model_path: &Path) -> Result<()> {
    let mut guard = model.lock().await;
    if guard.is_some() {
        return Ok(());
    }
    if !model_path.exists() {
        return Err(anyhow!(
            "Model not found: {}. Download it first.",
            model_path.display()
        ));
    }
    let path = model_path.to_string_lossy().to_string();
    let ctx = tokio::task::spawn_blocking(move || {
        whisper_rs::WhisperContext::new_with_params(
            &path,
            whisper_rs::WhisperContextParameters::default(),
        )
    })
    .await?
    .map_err(|e| anyhow!("Failed to load whisper model: {e:?}"))?;

    *guard = Some(Arc::new(ctx));
    eprintln!("[whisper] model loaded: {}", model_path.display());
    Ok(())
}

/// Run transcription on 16 kHz mono f32 audio. Returns trimmed text or empty string.
pub async fn transcribe(model: &WhisperModel, samples: Vec<f32>) -> Result<String> {
    if samples.len() < 1600 {
        // Less than 100ms — skip
        return Ok(String::new());
    }

    let ctx = {
        let guard = model.lock().await;
        guard
            .as_ref()
            .ok_or_else(|| anyhow!("Whisper model not loaded"))?
            .clone()
    };

    tokio::task::spawn_blocking(move || {
        let mut state = ctx
            .create_state()
            .map_err(|e| anyhow!("create_state: {e:?}"))?;

        let mut params = whisper_rs::FullParams::new(whisper_rs::SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(Some("en"));
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_special(false);
        params.set_translate(false);
        params.set_no_context(true);
        params.set_n_threads(2);

        state
            .full(params, &samples)
            .map_err(|e| anyhow!("whisper full: {e:?}"))?;

        let n = state.full_n_segments();
        let mut parts = Vec::new();
        for i in 0..n {
            if let Some(seg) = state.get_segment(i) {
                if let Ok(text) = seg.to_str() {
                    parts.push(text.to_string());
                }
            }
        }

        let result = parts.join(" ").trim().to_string();
        Ok::<String, anyhow::Error>(filter_hallucinations(&result))
    })
    .await?
}

/// Filter Whisper hallucinations (empty brackets, filler phrases).
fn filter_hallucinations(text: &str) -> String {
    let t = text.trim();
    // Whisper often emits these for silence
    if t.starts_with('[') && t.ends_with(']') {
        return String::new();
    }
    if t.starts_with('(') && t.ends_with(')') {
        return String::new();
    }
    // Common filler hallucinations
    match t {
        "Thanks for watching!"
        | "Thank you for watching!"
        | "Thank you."
        | "."
        | "" => String::new(),
        _ => t.to_string(),
    }
}

/// Download a Whisper GGML model from Hugging Face with progress events.
pub async fn download_model(
    model_name: &str,
    models_dir: &Path,
    app: &tauri::AppHandle,
) -> Result<PathBuf> {
    let filename = match model_name {
        "whisper-base" | "base" => "ggml-base.en.bin",
        _ => "ggml-tiny.en.bin",
    };
    let dest = models_dir.join(filename);
    if dest.exists() {
        return Ok(dest);
    }

    tokio::fs::create_dir_all(models_dir).await?;

    let url = format!(
        "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/{}",
        filename
    );

    eprintln!("[whisper] downloading {} from {}", filename, url);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(600))
        .build()?;
    let mut response = client.get(&url).send().await?;

    if !response.status().is_success() {
        return Err(anyhow!("Download failed: HTTP {}", response.status()));
    }

    let total = response.content_length().unwrap_or(0);
    let mut file = tokio::fs::File::create(&dest).await?;
    let mut downloaded: u64 = 0;

    while let Some(chunk) = response.chunk().await? {
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;
        let percent = if total > 0 {
            downloaded * 100 / total
        } else {
            0
        };
        let _ = app.emit(
            "whisper-download-progress",
            serde_json::json!({
                "model": model_name,
                "downloaded": downloaded,
                "total": total,
                "percent": percent,
                "done": false,
            }),
        );
    }

    file.flush().await?;
    let _ = app.emit(
        "whisper-download-progress",
        serde_json::json!({
            "model": model_name,
            "downloaded": downloaded,
            "total": total,
            "percent": 100,
            "done": true,
        }),
    );

    eprintln!("[whisper] downloaded {} to {}", filename, dest.display());
    Ok(dest)
}
