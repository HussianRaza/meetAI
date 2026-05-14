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
use tauri::{Manager, State};

pub struct AppState {
    pub pool: SqlitePool,
    pub data_dir: PathBuf,
    pub embed_model: EmbedModel,
    pub watcher_task: tokio::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
    // M3
    pub whisper_model: stt::WhisperModel,
    pub active_session: tokio::sync::Mutex<Option<ActiveSession>>,
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
        groq_key: cfg.groq_key,
    };

    let session = meeting::start_session(
        title,
        platform,
        state.pool.clone(),
        state.whisper_model.clone(),
        state.embed_model.clone(),
        state.data_dir.clone(),
        app,
        nudge_cfg,
    )
    .await
    .map_err(|e| e.to_string())?;

    let meeting_id = session.meeting_id.clone();
    *guard = Some(session);
    Ok(meeting_id)
}

#[tauri::command]
async fn meeting_stop(state: State<'_, AppState>) -> Result<String, String> {
    let mut guard = state.active_session.lock().await;
    let session = guard.take().ok_or("No active meeting")?;
    meeting::stop_session(session, state.pool.clone())
        .await
        .map_err(|e| e.to_string())
}

// ── Entry point ──────────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_sql::Builder::default().build())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let handle = app.handle().clone();
            tauri::async_runtime::block_on(async move {
                let pool = db::init(&handle).await.expect("db init failed");
                let data_dir = handle.path().app_data_dir().expect("no app data dir");

                // Recover meetings stuck in 'recording' from a crash
                meeting::session::recover_interrupted(&pool).await;

                handle.manage(AppState {
                    pool,
                    data_dir,
                    embed_model: kb::embed::new_handle(),
                    watcher_task: tokio::sync::Mutex::new(None),
                    whisper_model: stt::new_handle(),
                    active_session: tokio::sync::Mutex::new(None),
                });
            });
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
