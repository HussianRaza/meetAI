use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

const RMS_THRESHOLD: f32 = 0.018; // ~-35 dBFS — typical meeting audio level
const HOT_SECS: u64 = 10;         // sustained duration before emitting event
const POLL_SECS: u64 = 2;         // polling interval

pub struct AutoDetectHandle {
    stop: Arc<AtomicBool>,
}

impl AutoDetectHandle {
    pub fn stop(self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

pub fn start(app: AppHandle) -> AutoDetectHandle {
    let stop = Arc::new(AtomicBool::new(false));
    let stop_clone = stop.clone();

    thread::spawn(move || {
        run_loop(app, stop_clone);
    });

    AutoDetectHandle { stop }
}

fn run_loop(app: AppHandle, stop: Arc<AtomicBool>) {
    let host = cpal::default_host();

    let device = match host.default_input_device() {
        Some(d) => d,
        None => {
            eprintln!("[autodetect] no input device — auto-detect disabled");
            return;
        }
    };

    let supported = match device.default_input_config() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[autodetect] config error: {e}");
            return;
        }
    };

    let sample_rate = supported.sample_rate().0;
    let channels = supported.channels() as usize;
    let window_samples = sample_rate as usize * POLL_SECS as usize;

    let buf: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));
    let buf_cb = buf.clone();

    let err_fn = |e| eprintln!("[autodetect] stream error: {e}");
    let stream_cfg: cpal::StreamConfig = supported.clone().into();

    let stream = match supported.sample_format() {
        cpal::SampleFormat::F32 => device.build_input_stream(
            &stream_cfg,
            move |data: &[f32], _| push_mono(data, channels, &buf_cb),
            err_fn,
            None,
        ),
        cpal::SampleFormat::I16 => device.build_input_stream(
            &stream_cfg,
            move |data: &[i16], _| {
                let f: Vec<f32> = data.iter().map(|s| *s as f32 / 32_768.0).collect();
                push_mono(&f, channels, &buf_cb);
            },
            err_fn,
            None,
        ),
        cpal::SampleFormat::U16 => device.build_input_stream(
            &stream_cfg,
            move |data: &[u16], _| {
                let f: Vec<f32> = data
                    .iter()
                    .map(|s| (*s as f32 - 32_768.0) / 32_768.0)
                    .collect();
                push_mono(&f, channels, &buf_cb);
            },
            err_fn,
            None,
        ),
        fmt => {
            eprintln!("[autodetect] unsupported sample format {fmt:?}");
            return;
        }
    };

    let stream = match stream {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[autodetect] failed to build stream: {e}");
            return;
        }
    };
    if let Err(e) = stream.play() {
        eprintln!("[autodetect] stream.play() failed: {e}");
        return;
    }

    let mut hot_windows: u64 = 0;
    let mut emitted = false;

    loop {
        thread::sleep(Duration::from_secs(POLL_SECS));

        if stop.load(Ordering::Relaxed) {
            break;
        }

        let rms = {
            let b = buf.lock().unwrap_or_else(|e| e.into_inner());
            let window = if b.len() >= window_samples {
                &b[b.len() - window_samples..]
            } else {
                b.as_slice()
            };
            if window.is_empty() {
                0.0f32
            } else {
                let sq: f32 = window.iter().map(|s| s * s).sum();
                (sq / window.len() as f32).sqrt()
            }
        };

        if rms > RMS_THRESHOLD {
            hot_windows += 1;
            if hot_windows * POLL_SECS >= HOT_SECS && !emitted {
                let _ = app.emit("meeting-detected", ());
                emitted = true;
            }
        } else {
            hot_windows = 0;
            emitted = false;
        }
    }
    // `stream` dropped here — capture stops
}

fn push_mono(data: &[f32], channels: usize, buf: &Arc<Mutex<Vec<f32>>>) {
    let mono: Vec<f32> = data
        .chunks(channels)
        .map(|ch| ch.iter().sum::<f32>() / channels as f32)
        .collect();
    if let Ok(mut b) = buf.try_lock() {
        b.extend_from_slice(&mono);
        // Keep at most 12 seconds of audio
        let max = 48_000 * 12;
        let len = b.len();
        if len > max {
            b.drain(..len - max);
        }
    }
}
