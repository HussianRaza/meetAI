mod audio;
mod db;
mod kb;
mod llm;
mod meeting;
mod nudge;
mod settings;
mod stt;

use kb::{embed::EmbedModel, search::SearchResult};
use meeting::ActiveSession;
use sqlx::SqlitePool;
use std::path::PathBuf;
use std::sync::OnceLock;
use tauri::{Manager, State};

// ── Crash log ────────────────────────────────────────────────────────────────

static LOG_PATH: OnceLock<PathBuf> = OnceLock::new();

fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let msg = format!("{info}");
        eprintln!("[PANIC] {msg}");
        if let Some(path) = LOG_PATH.get() {
            use std::io::Write;
            if let Ok(mut f) =
                std::fs::OpenOptions::new().append(true).create(true).open(path)
            {
                let ts = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                let _ = writeln!(f, "[{ts}] [PANIC] {msg}");
            }
        }
    }));
}

pub struct AppState {
    pub pool: SqlitePool,
    pub data_dir: PathBuf,
    pub embed_model: EmbedModel,
    pub watcher_task: tokio::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
    // M3
    pub whisper_model: stt::WhisperModel,
    pub active_session: tokio::sync::Mutex<Option<ActiveSession>>,
    // M6
    pub job_tx: tokio::sync::mpsc::Sender<meeting::JobRequest>,
    // M8
    pub auto_detect: tokio::sync::Mutex<Option<audio::autodetect::AutoDetectHandle>>,
}

// ── Settings commands ────────────────────────────────────────────────────────

#[tauri::command]
async fn settings_get(state: State<'_, AppState>) -> Result<settings::Settings, String> {
    settings::get(&state.pool).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn settings_set(
    key: String,
    value: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    settings::set(&state.pool, &key, &value)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn groq_test_connection(key: String) -> Result<bool, String> {
    settings::test_groq(&key).await.map_err(|e| e.to_string())
}

// ── KB commands ──────────────────────────────────────────────────────────────

#[tauri::command]
async fn kb_index_start(
    folder: String,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let model_dir = state.data_dir.join("models");
    kb::embed::ensure_init(&state.embed_model, &model_dir)
        .await
        .map_err(|e| e.to_string())?;

    {
        let mut guard = state.watcher_task.lock().await;
        if let Some(h) = guard.take() {
            h.abort();
        }
    }

    let pool = state.pool.clone();
    let model = state.embed_model.clone();
    let app_clone = app.clone();

    kb::index::index_folder(&folder, &pool, &model, &app_clone)
        .await
        .map_err(|e| e.to_string())?;

    let handle = kb::watcher::start(folder, pool, model, app_clone)
        .await
        .map_err(|e| e.to_string())?;

    let mut guard = state.watcher_task.lock().await;
    *guard = Some(handle);

    Ok(())
}

#[tauri::command]
async fn kb_reindex_all(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let folder = settings::get(&state.pool)
        .await
        .map_err(|e| e.to_string())?
        .kb_folder;

    if folder.is_empty() {
        return Err("No KB folder configured".into());
    }

    let model_dir = state.data_dir.join("models");
    kb::embed::ensure_init(&state.embed_model, &model_dir)
        .await
        .map_err(|e| e.to_string())?;

    kb::index::index_folder(&folder, &state.pool, &state.embed_model, &app)
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
async fn kb_search(
    query: String,
    top_k: Option<usize>,
    state: State<'_, AppState>,
) -> Result<Vec<SearchResult>, String> {
    kb::search::search(&state.pool, &state.embed_model, &query, top_k.unwrap_or(5))
        .await
        .map_err(|e| e.to_string())
}

// ── Audio / Whisper commands ─────────────────────────────────────────────────

#[tauri::command]
async fn audio_devices_list() -> Vec<audio::mic::DeviceInfo> {
    audio::mic::list_devices()
}

#[tauri::command]
async fn whisper_model_status(state: State<'_, AppState>) -> Result<stt::WhisperStatus, String> {
    let cfg = settings::get(&state.pool)
        .await
        .map_err(|e| e.to_string())?;
    let models_dir = state.data_dir.join("models");
    let path = stt::model_path_for(&cfg.whisper_model, &models_dir);
    Ok(stt::WhisperStatus {
        ready: path.exists(),
        model_name: cfg.whisper_model,
        model_path: path.to_string_lossy().to_string(),
    })
}

#[tauri::command]
async fn whisper_download_model(
    model_name: String,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let models_dir = state.data_dir.join("models");
    stt::download_model(&model_name, &models_dir, &app)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

// ── Library commands ─────────────────────────────────────────────────────────

#[tauri::command]
async fn meetings_list(
    state: State<'_, AppState>,
) -> Result<Vec<meeting::MeetingRow>, String> {
    meeting::library::list_meetings(&state.pool)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn meeting_search(
    query: String,
    state: State<'_, AppState>,
) -> Result<Vec<meeting::MeetingRow>, String> {
    meeting::library::search_meetings(&state.pool, &query)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn chat_query(
    question: String,
    state: State<'_, AppState>,
) -> Result<meeting::ChatResponse, String> {
    let cfg = settings::get(&state.pool)
        .await
        .map_err(|e| e.to_string())?;

    let (context, sources) = meeting::library::build_chat_context(&state.pool, &question)
        .await
        .map_err(|e| e.to_string())?;

    if context.is_empty() {
        return Ok(meeting::ChatResponse {
            answer: "No relevant meeting transcripts found for this question. \
                     Record and process some meetings first."
                .to_string(),
            sources: vec![],
        });
    }

    let system_msg = "You are a meeting assistant. Answer the user's question using ONLY \
                      the provided meeting transcript excerpts. \
                      Cite the meeting name and timestamp when you reference a quote. \
                      Be concise and factual.";
    let user_msg = format!(
        "Meeting transcripts:\n{context}\n\nQuestion: {question}"
    );
    let messages = vec![
        serde_json::json!({"role": "system", "content": system_msg}),
        serde_json::json!({"role": "user",   "content": user_msg}),
    ];

    let client = reqwest::Client::new();
    let answer = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        llm::chat(&client, &cfg.groq_key, llm::MODEL_LIVE, messages, 600),
    )
    .await
    .map_err(|_| "Chat timed out after 30 s".to_string())?
    .map_err(|e| e.to_string())?;

    Ok(meeting::ChatResponse { answer, sources })
}

// ── Post-meeting commands ─────────────────────────────────────────────────────

#[tauri::command]
async fn meeting_get(
    id: String,
    state: State<'_, AppState>,
) -> Result<meeting::MeetingDetail, String> {
    meeting::library::get_meeting(&state.pool, &id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn action_item_toggle(
    id: i64,
    done: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    meeting::library::toggle_action_item(&state.pool, id, done)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn meeting_notes_save(
    id: String,
    notes: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    meeting::library::save_notes(&state.pool, &id, &notes)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn meeting_export_markdown(
    id: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    meeting::library::export_markdown(&state.pool, &id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn meeting_regenerate_summary(
    id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .job_tx
        .send(meeting::JobRequest { meeting_id: id, kind: meeting::JobKind::Summarize })
        .await
        .map_err(|e| e.to_string())
}

// ── Meeting commands ─────────────────────────────────────────────────────────

#[tauri::command]
async fn meeting_start(
    title: String,
    platform: Option<String>,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    let mut guard = state.active_session.lock().await;
    if guard.is_some() {
        return Err("A meeting is already in progress".into());
    }

    let cfg = settings::get(&state.pool)
        .await
        .map_err(|e| e.to_string())?;
    let models_dir = state.data_dir.join("models");
    let model_path = stt::model_path_for(&cfg.whisper_model, &models_dir);
    stt::ensure_loaded(&state.whisper_model, &model_path)
        .await
        .map_err(|e| e.to_string())?;

    let nudge_cfg = nudge::NudgeSettings {
        enabled: cfg.nudge_enabled,
        ai_suggestions: cfg.ai_suggestions_enabled,
        interval_secs: cfg.nudge_interval_secs as u64,
        threshold: cfg.nudge_threshold,
        groq_key: cfg.groq_key.clone(),
    };

    let session = meeting::start_session(
        title,
        platform,
        state.pool.clone(),
        state.whisper_model.clone(),
        state.embed_model.clone(),
        state.data_dir.clone(),
        app.clone(),
        nudge_cfg,
    )
    .await
    .map_err(|e| e.to_string())?;

    let meeting_id = session.meeting_id.clone();
    *guard = Some(session);

    // Protect both windows from screen capture during recording (no-op on Linux)
    if cfg.screen_share_protection {
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.set_content_protected(true);
        }
        if let Some(w) = app.get_webview_window("overlay") {
            let _ = w.set_content_protected(true);
        }
    }

    Ok(meeting_id)
}

#[tauri::command]
async fn meeting_stop(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    let mut guard = state.active_session.lock().await;
    let session = guard.take().ok_or("No active meeting")?;
    let meeting_id = meeting::stop_session(session, state.pool.clone())
        .await
        .map_err(|e| e.to_string())?;

    // Enqueue post-meeting pipeline
    let _ = state
        .job_tx
        .send(meeting::JobRequest {
            meeting_id: meeting_id.clone(),
            kind: meeting::JobKind::Embed,
        })
        .await;
    let _ = state
        .job_tx
        .send(meeting::JobRequest {
            meeting_id: meeting_id.clone(),
            kind: meeting::JobKind::Summarize,
        })
        .await;

    // Remove content protection after recording
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.set_content_protected(false);
    }
    if let Some(w) = app.get_webview_window("overlay") {
        let _ = w.set_content_protected(false);
        let _ = w.hide();
    }

    Ok(meeting_id)
}

// ── Telemetry / info commands ─────────────────────────────────────────────────

#[tauri::command]
async fn log_file_path(state: State<'_, AppState>) -> Result<String, String> {
    let path = state.data_dir.join("logs").join("meetai.log");
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
async fn mcp_snippet(state: State<'_, AppState>) -> Result<String, String> {
    let db_path = state.data_dir.join("meetai.db");

    // Best-effort: find meetai-mcp next to the running executable
    let bin_path = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("meetai-mcp")))
        .unwrap_or_else(|| PathBuf::from("meetai-mcp"));

    let snippet = serde_json::json!({
        "mcpServers": {
            "meetai": {
                "command": bin_path.to_string_lossy(),
                "args": ["--db", db_path.to_string_lossy()]
            }
        }
    });

    Ok(serde_json::to_string_pretty(&snippet).unwrap())
}

// ── Auto-detect commands ─────────────────────────────────────────────────────

#[tauri::command]
async fn auto_start_enable(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let mut guard = state.auto_detect.lock().await;
    if guard.is_none() {
        *guard = Some(audio::autodetect::start(app));
    }
    Ok(())
}

#[tauri::command]
async fn auto_start_disable(state: State<'_, AppState>) -> Result<(), String> {
    let mut guard = state.auto_detect.lock().await;
    if let Some(h) = guard.take() {
        h.stop();
    }
    Ok(())
}

// ── Overlay commands ─────────────────────────────────────────────────────────

#[tauri::command]
async fn overlay_show(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("overlay") {
        w.show().map_err(|e| e.to_string())?;
        w.set_focus().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
async fn overlay_hide(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("overlay") {
        w.hide().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
async fn overlay_toggle(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("overlay") {
        if w.is_visible().map_err(|e| e.to_string())? {
            w.hide().map_err(|e| e.to_string())?;
        } else {
            w.show().map_err(|e| e.to_string())?;
            w.set_focus().map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

// ── Entry point ──────────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    install_panic_hook();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_sql::Builder::default().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    use tauri_plugin_global_shortcut::ShortcutState;
                    if event.state == ShortcutState::Pressed {
                        if let Some(w) = app.get_webview_window("overlay") {
                            if let Ok(visible) = w.is_visible() {
                                if visible {
                                    let _ = w.hide();
                                } else {
                                    let _ = w.show();
                                    let _ = w.set_focus();
                                }
                            }
                        }
                    }
                })
                .build(),
        )
        .setup(|app| {
            let handle = app.handle().clone();
            tauri::async_runtime::block_on(async move {
                let pool = db::init(&handle).await.expect("db init failed");
                let data_dir = handle.path().app_data_dir().expect("no app data dir");

                // Initialise file logger (writes WARN+ to logs/meetai.log)
                {
                    use simplelog::{Config, LevelFilter, WriteLogger};
                    let log_dir = data_dir.join("logs");
                    std::fs::create_dir_all(&log_dir).ok();
                    let log_path = log_dir.join("meetai.log");
                    if let Ok(file) = std::fs::OpenOptions::new()
                        .append(true)
                        .create(true)
                        .open(&log_path)
                    {
                        WriteLogger::init(LevelFilter::Warn, Config::default(), file).ok();
                    }
                    LOG_PATH.set(log_path).ok();
                }

                // Recover meetings stuck in 'recording' from a crash
                meeting::session::recover_interrupted(&pool).await;

                // Start job queue task
                let embed_model = kb::embed::new_handle();
                let (job_tx, job_rx) =
                    tokio::sync::mpsc::channel::<meeting::JobRequest>(64);
                {
                    let pool2 = pool.clone();
                    let embed2 = embed_model.clone();
                    let data2 = data_dir.clone();
                    let app2 = handle.clone();
                    tokio::spawn(async move {
                        meeting::jobs::run_queue(job_rx, pool2, embed2, data2, app2).await;
                    });
                }

                // Start auto-detect if the user had it enabled previously
                let auto_detect_handle = {
                    let cfg = settings::get(&pool).await.unwrap_or_default();
                    if cfg.auto_start {
                        Some(audio::autodetect::start(handle.clone()))
                    } else {
                        None
                    }
                };

                handle.manage(AppState {
                    pool,
                    data_dir,
                    embed_model,
                    watcher_task: tokio::sync::Mutex::new(None),
                    whisper_model: stt::new_handle(),
                    active_session: tokio::sync::Mutex::new(None),
                    job_tx,
                    auto_detect: tokio::sync::Mutex::new(auto_detect_handle),
                });
            });

            // Create overlay window (hidden; shown during recording via shortcut or meeting_start)
            let overlay_url = if cfg!(debug_assertions) {
                tauri::WebviewUrl::External(
                    "http://localhost:1420/overlay.html"
                        .parse()
                        .expect("invalid overlay URL"),
                )
            } else {
                tauri::WebviewUrl::App("overlay.html".into())
            };

            tauri::WebviewWindowBuilder::new(app, "overlay", overlay_url)
                .title("MeetAI Overlay")
                .decorations(false)
                .transparent(true)
                .always_on_top(true)
                .skip_taskbar(true)
                .inner_size(380.0, 220.0)
                .position(1540.0, 860.0)
                .visible(false)
                .build()
                .expect("failed to create overlay window");

            // Register Ctrl+Shift+O global shortcut (best-effort; may fail on Wayland)
            {
                use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut};
                let shortcut =
                    Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyO);
                if let Err(e) = app.handle().global_shortcut().register(shortcut) {
                    eprintln!("Warning: could not register global shortcut: {e}");
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            settings_get,
            settings_set,
            groq_test_connection,
            kb_index_start,
            kb_reindex_all,
            kb_search,
            audio_devices_list,
            whisper_model_status,
            whisper_download_model,
            meeting_start,
            meeting_stop,
            meetings_list,
            meeting_search,
            chat_query,
            meeting_get,
            action_item_toggle,
            meeting_notes_save,
            meeting_export_markdown,
            meeting_regenerate_summary,
            overlay_show,
            overlay_hide,
            overlay_toggle,
            auto_start_enable,
            auto_start_disable,
            log_file_path,
            mcp_snippet,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
