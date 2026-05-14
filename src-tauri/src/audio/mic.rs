use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use serde::Serialize;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tokio::sync::mpsc;

use super::{to_mono, AudioSource};

#[derive(Debug, Serialize)]
pub struct DeviceInfo {
    pub name: String,
    pub kind: String, // "input" or "monitor"
}

pub fn list_devices() -> Vec<DeviceInfo> {
    let host = cpal::default_host();
    let mut devices = Vec::new();

    if let Ok(inputs) = host.input_devices() {
        for d in inputs {
            if let Ok(name) = d.name() {
                let kind = if name.to_lowercase().contains("monitor") {
                    "monitor"
                } else {
                    "input"
                };
                devices.push(DeviceInfo {
                    name,
                    kind: kind.to_string(),
                });
            }
        }
    }
    devices
}

/// Run on a dedicated std::thread. Blocks until `stop` is set.
pub fn capture_loop(tx: mpsc::Sender<(AudioSource, Vec<f32>, u32)>, stop: Arc<AtomicBool>) {
    let host = cpal::default_host();

    let device = match host.default_input_device() {
        Some(d) => d,
        None => {
            eprintln!("[mic] no default input device");
            return;
        }
    };

    let config = match device.default_input_config() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[mic] config error: {e}");
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
                    let _ = tx2.try_send((AudioSource::Mic, mono, sample_rate));
                },
                |e| eprintln!("[mic] stream error: {e}"),
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
                    let _ = tx2.try_send((AudioSource::Mic, mono, sample_rate));
                },
                |e| eprintln!("[mic] stream error: {e}"),
                None,
            )
        }
        fmt => {
            eprintln!("[mic] unsupported sample format: {fmt:?}");
            return;
        }
    };

    let stream = match build_result {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[mic] build stream error: {e}");
            return;
        }
    };

    if let Err(e) = stream.play() {
        eprintln!("[mic] play error: {e}");
        return;
    }

    eprintln!("[mic] capturing at {}Hz, {}ch", sample_rate, channels);

    while !stop.load(Ordering::Relaxed) {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    // stream dropped here → capture stops
    eprintln!("[mic] capture stopped");
}

/// Check if the default input device is available.
pub fn default_device_name() -> Result<String> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| anyhow::anyhow!("no default input device"))?;
    device
        .name()
        .map_err(|e| anyhow::anyhow!("device name error: {e}"))
}
