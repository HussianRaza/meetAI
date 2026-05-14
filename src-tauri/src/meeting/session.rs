use anyhow::Result;
use serde::Serialize;
use sqlx::SqlitePool;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tauri::Emitter;
use tokio::sync::mpsc;

use crate::audio::{self, vad::EnergyVad, AudioSource};
use crate::kb::embed::EmbedModel;
use crate::nudge::{self, NudgeSettings};
use crate::stt;

// ── Public structs ────────────────────────────────────────────────────────────

pub struct ActiveSession {
    pub meeting_id: String,
    stop: Arc<AtomicBool>,
    engine: tokio::task::JoinHandle<()>,
    nudge: Option<tokio::task::JoinHandle<()>>,
}

#[derive(Debug, Serialize, Clone)]
pub struct TranscriptSegmentEvent {
    pub meeting_id: String,
    pub source: String,
    pub text: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub is_final: bool,
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Create a meeting record in the DB and start the audio + nudge pipelines.
pub async fn start_session(
    title: String,
    platform: Option<String>,
    pool: SqlitePool,
    whisper_model: stt::WhisperModel,
    embed_model: EmbedModel,
    data_dir: PathBuf,
    app: tauri::AppHandle,
    nudge_settings: NudgeSettings,
) -> Result<ActiveSession> {
    let meeting_id = uuid::Uuid::new_v4().to_string();
    let now_ms = now_millis();

    sqlx::query(
        "INSERT INTO meetings (id, title, platform, status, started_at) VALUES (?, ?, ?, 'recording', ?)",
    )
    .bind(&meeting_id)
    .bind(&title)
    .bind(platform.as_deref())
    .bind(now_ms)
    .execute(&pool)
    .await?;

    let stop = Arc::new(AtomicBool::new(false));

    let nudge = nudge::start(
        meeting_id.clone(),
        nudge_settings,
        pool.clone(),
        embed_model,
        stop.clone(),
        app.clone(),
    );

    let engine = launch_engine(
        meeting_id.clone(),
        stop.clone(),
        pool,
        whisper_model,
        data_dir,
        app,
    );

    Ok(ActiveSession { meeting_id, stop, engine, nudge })
}

/// Stop recording: signal the pipeline, wait for it, update the DB record.
pub async fn stop_session(session: ActiveSession, pool: SqlitePool) -> Result<String> {
    let meeting_id = session.meeting_id.clone();

    // Signal all threads/tasks to stop
    session.stop.store(true, Ordering::Relaxed);

    // Stop nudge engine (fast — it polls every 100 ms)
    if let Some(nudge) = session.nudge {
        let _ = tokio::time::timeout(std::time::Duration::from_secs(2), nudge).await;
    }

    // Give the audio engine up to 8 s to flush pending Whisper jobs
    let _ = tokio::time::timeout(
        std::time::Duration::from_secs(8),
        session.engine,
    )
    .await;

    let ended_at = now_millis();
    let (started_at,): (i64,) = sqlx::query_as("SELECT started_at FROM meetings WHERE id=?")
        .bind(&meeting_id)
        .fetch_one(&pool)
        .await
        .unwrap_or((ended_at,));
    let duration_ms = ended_at - started_at;

    sqlx::query(
        "UPDATE meetings SET status='processing', ended_at=?, duration_ms=? WHERE id=?",
    )
    .bind(ended_at)
    .bind(duration_ms)
    .bind(&meeting_id)
    .execute(&pool)
    .await?;

    Ok(meeting_id)
}

// ── Crash recovery ────────────────────────────────────────────────────────────

/// On startup: any meeting still in 'recording' status was interrupted → mark processing.
pub async fn recover_interrupted(pool: &SqlitePool) {
    let result = sqlx::query(
        "UPDATE meetings SET status='processing' WHERE status='recording'",
    )
    .execute(pool)
    .await;
    if let Err(e) = result {
        eprintln!("[session] crash recovery error: {e}");
    }
}

// ── Engine ────────────────────────────────────────────────────────────────────

fn launch_engine(
    meeting_id: String,
    stop: Arc<AtomicBool>,
    pool: SqlitePool,
    whisper_model: stt::WhisperModel,
    data_dir: PathBuf,
    app: tauri::AppHandle,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        // Channel: mic/sys threads → pipeline task (large buffer to survive brief spikes)
        let (audio_tx, mut audio_rx) =
            mpsc::channel::<(AudioSource, Vec<f32>, u32)>(2048);

        // Start mic capture thread
        let mic_tx = audio_tx.clone();
        let mic_stop = stop.clone();
        std::thread::spawn(move || audio::mic::capture_loop(mic_tx, mic_stop));

        // Start system audio capture thread (no-op if no monitor found)
        let sys_tx = audio_tx.clone();
        let sys_stop = stop.clone();
        std::thread::spawn(move || audio::system::capture_loop(sys_tx, sys_stop));

        // Drop the original sender so channel closes when both threads exit
        drop(audio_tx);

        // Open WAV writer on a sync thread
        let wav_dir = data_dir.join("recordings");
        let wav_path = wav_dir.join(format!("{}.wav", meeting_id));
        tokio::fs::create_dir_all(&wav_dir).await.ok();

        let (wav_tx, wav_rx) = std::sync::mpsc::sync_channel::<Vec<f32>>(512);
        std::thread::spawn(move || {
            let spec = hound::WavSpec {
                channels: 1,
                sample_rate: 16000,
                bits_per_sample: 32,
                sample_format: hound::SampleFormat::Float,
            };
            if let Ok(mut w) = hound::WavWriter::create(&wav_path, spec) {
                for chunk in wav_rx.iter() {
                    for s in chunk {
                        let _ = w.write_sample(s);
                    }
                }
                let _ = w.finalize();
            }
        });

        // Per-source VAD state (both always at 16 kHz after resampling)
        let mut mic_vad = EnergyVad::new(16000);
        let mut sys_vad = EnergyVad::new(16000);

        // Process loop
        loop {
            let stopped = stop.load(Ordering::Relaxed);

            // Drain the channel (non-blocking when stopped)
            let received = if stopped {
                audio_rx.try_recv().ok().map(Some)
            } else {
                match tokio::time::timeout(
                    std::time::Duration::from_millis(100),
                    audio_rx.recv(),
                )
                .await
                {
                    Ok(msg) => Some(msg),
                    Err(_) => None, // timeout — loop again to check stop flag
                }
            };

            match received {
                Some(Some((source, samples, rate))) => {
                    let resampled = audio::resample_to_16k(&samples, rate);
                    // Feed WAV writer
                    let _ = wav_tx.try_send(resampled.clone());

                    // VAD
                    let vad = match source {
                        AudioSource::Mic => &mut mic_vad,
                        AudioSource::System => &mut sys_vad,
                    };
                    let segments = vad.process(&resampled);
                    dispatch_segments(
                        segments,
                        &source,
                        &meeting_id,
                        &whisper_model,
                        &pool,
                        &app,
                    )
                    .await;
                }
                Some(None) => {
                    // Channel closed (both capture threads exited)
                    break;
                }
                None => {
                    // Timeout or nothing while stopped
                    if stopped {
                        break;
                    }
                }
            }
        }

        // Flush in-progress speech
        for (vad, source) in [
            (&mut mic_vad, AudioSource::Mic),
            (&mut sys_vad, AudioSource::System),
        ] {
            if let Some(seg) = vad.flush() {
                dispatch_one(seg, &source, &meeting_id, &whisper_model, &pool, &app).await;
            }
        }

        // Drop WAV sender → WAV writer thread finalises the file
        drop(wav_tx);
        eprintln!("[session] engine exited for {meeting_id}");
    })
}

// ── Helpers ───────────────────────────────────────────────────────────────────

async fn dispatch_segments(
    segments: Vec<audio::vad::SpeechSegment>,
    source: &AudioSource,
    meeting_id: &str,
    model: &stt::WhisperModel,
    pool: &SqlitePool,
    app: &tauri::AppHandle,
) {
    for seg in segments {
        dispatch_one(seg, source, meeting_id, model, pool, app).await;
    }
}

async fn dispatch_one(
    seg: audio::vad::SpeechSegment,
    source: &AudioSource,
    meeting_id: &str,
    model: &stt::WhisperModel,
    pool: &SqlitePool,
    app: &tauri::AppHandle,
) {
    let source_label = match source {
        AudioSource::Mic => "you",
        AudioSource::System => "speaker",
    }
    .to_string();
    let mid = meeting_id.to_string();
    let model = model.clone();
    let pool = pool.clone();
    let app = app.clone();
    let start_ms = seg.start_ms;
    let end_ms = seg.end_ms;

    tokio::spawn(async move {
        match stt::transcribe(&model, seg.samples).await {
            Ok(text) if !text.is_empty() => {
                let event = TranscriptSegmentEvent {
                    meeting_id: mid.clone(),
                    source: source_label.clone(),
                    text: text.clone(),
                    start_ms,
                    end_ms,
                    is_final: true,
                };
                let _ = app.emit("transcript-segment", &event);

                let created_at = now_millis();
                let _ = sqlx::query(
                    "INSERT INTO transcript_segments \
                     (meeting_id, source, text, start_ms, end_ms, is_final, created_at) \
                     VALUES (?, ?, ?, ?, ?, 1, ?)",
                )
                .bind(&mid)
                .bind(&source_label)
                .bind(&text)
                .bind(start_ms as i64)
                .bind(end_ms as i64)
                .bind(created_at)
                .execute(&pool)
                .await;
            }
            Ok(_) => {} // empty — filtered hallucination or silence
            Err(e) => eprintln!("[whisper] transcription error: {e}"),
        }
    });
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}
