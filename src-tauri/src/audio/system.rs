/// Linux system audio capture via PulseAudio/PipeWire monitor source.
///
/// On Linux with PulseAudio or PipeWire-PulseAudio, monitor sources are
/// exposed as ALSA input devices with "monitor" in their name.
/// If no monitor device is found, system audio capture is silently skipped.
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tokio::sync::mpsc;

use super::{to_mono, AudioSource};

fn find_monitor_device() -> Option<cpal::Device> {
    let host = cpal::default_host();
    host.input_devices()
        .ok()?
        .find(|d| {
            d.name()
                .map(|n| n.to_lowercase().contains("monitor"))
                .unwrap_or(false)
        })
}

/// Run on a dedicated std::thread. Blocks until `stop` is set.
/// If no monitor source is found, returns immediately (no-op).
pub fn capture_loop(tx: mpsc::Sender<(AudioSource, Vec<f32>, u32)>, stop: Arc<AtomicBool>) {
    let device = match find_monitor_device() {
        Some(d) => d,
        None => {
            eprintln!("[system] no monitor source found — system audio capture disabled");
            return;
        }
    };

    let name = device.name().unwrap_or_default();
    let config = match device.default_input_config() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[system] config error for {name}: {e}");
            return;
        }
    };

    let sample_rate = config.sample_rate().0;
    let channels = config.channels() as usize;

    let build_result = match config.sample_format() {
        cpal::SampleFormat::F32 => {
            let tx2 = tx.clone();
            let stop2 = stop.clone();
            device.build_input_stream(
                &config.into(),
                move |data: &[f32], _| {
                    if stop2.load(Ordering::Relaxed) {
                        return;
                    }
                    let mono = to_mono(data, channels);
                    let _ = tx2.try_send((AudioSource::System, mono, sample_rate));
                },
                |e| eprintln!("[system] stream error: {e}"),
                None,
            )
        }
        cpal::SampleFormat::I16 => {
            let tx2 = tx.clone();
            let stop2 = stop.clone();
            device.build_input_stream(
                &config.into(),
                move |data: &[i16], _| {
                    if stop2.load(Ordering::Relaxed) {
                        return;
                    }
                    let f32s: Vec<f32> = data
                        .iter()
                        .map(|&s| s as f32 / i16::MAX as f32)
                        .collect();
                    let mono = to_mono(&f32s, channels);
                    let _ = tx2.try_send((AudioSource::System, mono, sample_rate));
                },
                |e| eprintln!("[system] stream error: {e}"),
                None,
            )
        }
        fmt => {
            eprintln!("[system] unsupported sample format: {fmt:?}");
            return;
        }
    };

    let stream = match build_result {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[system] build stream error: {e}");
            return;
        }
    };

    if let Err(e) = stream.play() {
        eprintln!("[system] play error: {e}");
        return;
    }

    eprintln!("[system] capturing monitor '{name}' at {}Hz, {}ch", sample_rate, channels);

    while !stop.load(Ordering::Relaxed) {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    eprintln!("[system] capture stopped");
}
