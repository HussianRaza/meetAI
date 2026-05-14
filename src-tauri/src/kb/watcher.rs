use anyhow::Result;
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use sqlx::SqlitePool;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tauri::Emitter;
use tokio::sync::mpsc;

use super::{embed::EmbedModel, index::index_single_file};

/// Starts a file watcher on `folder`. Returns a JoinHandle that keeps the watcher alive.
/// Changed files are debounced 500ms then re-indexed.
pub async fn start(
    folder: String,
    pool: SqlitePool,
    model: EmbedModel,
    app: tauri::AppHandle,
) -> Result<tokio::task::JoinHandle<()>> {
    let (tx, mut rx) = mpsc::channel::<PathBuf>(256);

    // Spawn the watcher on a blocking thread to avoid blocking tokio
    let folder_path = PathBuf::from(&folder);
    let tx_clone = tx.clone();

    std::thread::spawn(move || {
        let mut watcher = RecommendedWatcher::new(
            move |res: notify::Result<notify::Event>| {
                if let Ok(event) = res {
                    match event.kind {
                        EventKind::Create(_)
                        | EventKind::Modify(_)
                        | EventKind::Remove(_) => {
                            for path in event.paths {
                                let _ = tx_clone.blocking_send(path);
                            }
                        }
                        _ => {}
                    }
                }
            },
            Config::default(),
        )
        .expect("watcher init failed");

        if let Err(e) = watcher.watch(&folder_path, RecursiveMode::Recursive) {
            eprintln!("[watcher] failed to watch {}: {}", folder_path.display(), e);
            return;
        }

        // Park the thread indefinitely — watcher keeps running until thread is dropped
        loop {
            std::thread::sleep(Duration::from_secs(60));
        }
    });

    // Tokio task: debounce and reindex
    let handle = tokio::spawn(async move {
        let mut pending: Vec<(PathBuf, Instant)> = Vec::new();

        loop {
            // Drain available events
            loop {
                match rx.try_recv() {
                    Ok(path) => {
                        // Remove existing entry for same path then push
                        pending.retain(|(p, _)| p != &path);
                        pending.push((path, Instant::now()));
                    }
                    Err(_) => break,
                }
            }

            // Process debounced files (> 500ms old)
            let now = Instant::now();
            let ready: Vec<PathBuf> = pending
                .iter()
                .filter(|(_, t)| now.duration_since(*t) >= Duration::from_millis(500))
                .map(|(p, _)| p.clone())
                .collect();

            for path in &ready {
                pending.retain(|(p, _)| p != path);
                if let Err(e) = index_single_file(path, &pool, &model).await {
                    eprintln!("[watcher] reindex {:?}: {}", path, e);
                } else {
                    let _ = app.emit(
                        "kb-index-progress",
                        serde_json::json!({
                            "current": 1, "total": 1,
                            "file": path.file_name().unwrap_or_default().to_string_lossy(),
                            "done": true
                        }),
                    );
                }
            }

            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    });

    Ok(handle)
}
