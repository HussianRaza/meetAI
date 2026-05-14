/// Linux system audio capture via PulseAudio/PipeWire monitor source.
///
/// Primary path:  cpal ALSA — works when the monitor is exposed as an ALSA input
///                             (classic PulseAudio or some PipeWire-ALSA setups).
/// Fallback path: `parec` subprocess — reliable on PipeWire-pulse (Arch Linux default).
///                Reads f32le mono at 16 kHz from the monitor source.
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tokio::sync::mpsc;

use super::{to_mono, AudioSource};

// ── cpal path ─────────────────────────────────────────────────────────────────

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

fn capture_via_cpal(
    device: cpal::Device,
    tx: mpsc::Sender<(AudioSource, Vec<f32>, u32)>,
    stop: Arc<AtomicBool>,
) {
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
                    let _ = tx2.try_send((AudioSource::System, to_mono(data, channels), sample_rate));
                },
                |e| eprintln!("[system/cpal] stream error: {e}"),
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
                    let f32s: Vec<f32> =
                        data.iter().map(|&s| s as f32 / i16::MAX as f32).collect();
                    let _ = tx2.try_send((AudioSource::System, to_mono(&f32s, channels), sample_rate));
                },
                |e| eprintln!("[system/cpal] stream error: {e}"),
                None,
            )
        }
        fmt => {
            eprintln!("[system/cpal] unsupported sample format: {fmt:?}");
            return;
        }
    };

    let stream = match build_result {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[system/cpal] build stream error: {e}");
            return;
        }
    };
    if let Err(e) = stream.play() {
        eprintln!("[system/cpal] play error: {e}");
        return;
    }

    eprintln!(
        "[system/cpal] capturing monitor '{name}' at {}Hz, {}ch",
        sample_rate, channels
    );

    while !stop.load(Ordering::Relaxed) {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    eprintln!("[system/cpal] capture stopped");
}

// ── parec (PipeWire-pulse) path ───────────────────────────────────────────────

/// Find the main soundcard monitor source via `pactl list sources short`.
/// Prefers non-Bluetooth monitors.
fn find_pa_monitor() -> Option<String> {
    let out = Command::new("pactl")
        .args(["list", "sources", "short"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);

    // Collect all monitor sources; sort so non-BT comes first
    let mut candidates: Vec<String> = text
        .lines()
        .filter(|l| l.contains("monitor"))
        .filter_map(|l| l.split_whitespace().nth(1).map(str::to_owned))
        .collect();
    candidates.sort_by_key(|s| if s.contains("bluez") { 1usize } else { 0 });
    candidates.into_iter().next()
}

fn capture_via_parec(
    tx: mpsc::Sender<(AudioSource, Vec<f32>, u32)>,
    stop: Arc<AtomicBool>,
) {
    let monitor = match find_pa_monitor() {
        Some(m) => m,
        None => {
            eprintln!("[system/parec] no monitor source found via pactl");
            return;
        }
    };

    eprintln!("[system/parec] capturing from {monitor}");

    let mut child = match Command::new("parec")
        .args([
            "--device",
            &monitor,
            "--format=float32le",
            "--rate=16000",
            "--channels=1",
            "--latency-msec=50",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[system/parec] failed to spawn: {e}");
            return;
        }
    };

    let mut stdout = child.stdout.take().expect("parec stdout");
    // 4096 bytes = 1024 f32 samples = 64 ms at 16 kHz mono
    let mut buf = vec![0u8; 4096];

    loop {
        if stop.load(Ordering::Relaxed) {
            let _ = child.kill();
            break;
        }

        match stdout.read(&mut buf) {
            Ok(0) => {
                eprintln!("[system/parec] EOF");
                break;
            }
            Ok(n) => {
                let n_aligned = n & !3; // round down to multiple of 4
                if n_aligned == 0 {
                    continue;
                }
                let samples: Vec<f32> = buf[..n_aligned]
                    .chunks_exact(4)
                    .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                    .collect();
                let _ = tx.try_send((AudioSource::System, samples, 16000));
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => {
                eprintln!("[system/parec] read error: {e}");
                break;
            }
        }
    }

    let _ = child.wait();
    eprintln!("[system/parec] capture stopped");
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Run on a dedicated std::thread. Blocks until `stop` is set.
/// Tries cpal ALSA monitor first; falls back to `parec` subprocess for PipeWire.
pub fn capture_loop(tx: mpsc::Sender<(AudioSource, Vec<f32>, u32)>, stop: Arc<AtomicBool>) {
    if let Some(device) = find_monitor_device() {
        capture_via_cpal(device, tx, stop);
    } else {
        capture_via_parec(tx, stop);
    }
}
