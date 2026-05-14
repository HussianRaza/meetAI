pub mod whisper;

pub use whisper::{
    WhisperModel, WhisperStatus, download_model, ensure_loaded, model_path_for, new_handle,
    transcribe,
};
