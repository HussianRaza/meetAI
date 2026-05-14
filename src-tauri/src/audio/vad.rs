/// Energy-based VAD (Voice Activity Detection).
///
/// Processes 16 kHz mono audio in 30ms chunks. Accumulates speech into
/// segments which are returned when silence exceeds the redemption window.
use std::collections::VecDeque;

pub struct SpeechSegment {
    pub samples: Vec<f32>, // 16 kHz mono
    pub start_ms: u64,
    pub end_ms: u64,
}

pub struct EnergyVad {
    sample_rate: u32,
    threshold_on: f32,         // RMS to start speech (0.015)
    threshold_off: f32,        // RMS to end speech — lower for hysteresis (0.008)
    redemption_samples: usize, // silence window before segment ends (~1.5 s)
    min_speech_samples: usize, // minimum speech length to emit (~250 ms)

    in_speech: bool,
    speech_buffer: Vec<f32>,
    silent_samples: usize,
    samples_processed: usize,  // total samples seen so far
    speech_start_sample: usize,

    pre_buffer: VecDeque<f32>, // rolling pre-speech context (~200 ms)
    pre_buffer_max: usize,
}

impl EnergyVad {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate,
            threshold_on: 0.015,
            threshold_off: 0.008,
            redemption_samples: (sample_rate as f32 * 1.5) as usize,
            min_speech_samples: (sample_rate as f32 * 0.25) as usize,
            in_speech: false,
            speech_buffer: Vec::new(),
            silent_samples: 0,
            samples_processed: 0,
            speech_start_sample: 0,
            pre_buffer: VecDeque::new(),
            pre_buffer_max: (sample_rate as f32 * 0.2) as usize, // 200 ms
        }
    }

    fn rms(samples: &[f32]) -> f32 {
        if samples.is_empty() {
            return 0.0;
        }
        let sum_sq: f32 = samples.iter().map(|&s| s * s).sum();
        (sum_sq / samples.len() as f32).sqrt()
    }

    fn sample_to_ms(&self, n: usize) -> u64 {
        (n as u64 * 1000) / self.sample_rate as u64
    }

    /// Process a batch of 16 kHz mono samples. Returns any completed segments.
    pub fn process(&mut self, samples: &[f32]) -> Vec<SpeechSegment> {
        let mut completed = Vec::new();
        // 30 ms chunks at 16 kHz = 480 samples
        let chunk_size = ((self.sample_rate as usize * 30) / 1000).max(64);
        let mut i = 0;

        while i < samples.len() {
            let chunk_end = (i + chunk_size).min(samples.len());
            let chunk = &samples[i..chunk_end];
            let chunk_start_abs = self.samples_processed + i;
            let chunk_end_abs = self.samples_processed + chunk_end;
            let energy = Self::rms(chunk);

            if !self.in_speech {
                if energy >= self.threshold_on {
                    // Speech started — include pre-buffer as pre-roll
                    let pre_len = self.pre_buffer.len();
                    self.speech_start_sample = chunk_start_abs.saturating_sub(pre_len);
                    self.speech_buffer.extend(self.pre_buffer.drain(..));
                    self.speech_buffer.extend_from_slice(chunk);
                    self.in_speech = true;
                    self.silent_samples = 0;
                } else {
                    // Not speech — add to rolling pre-buffer
                    self.pre_buffer.extend(chunk.iter().copied());
                    while self.pre_buffer.len() > self.pre_buffer_max {
                        self.pre_buffer.pop_front();
                    }
                }
            } else {
                self.speech_buffer.extend_from_slice(chunk);
                if energy < self.threshold_off {
                    self.silent_samples += chunk.len();
                } else {
                    self.silent_samples = 0;
                }
                if self.silent_samples >= self.redemption_samples {
                    // Speech ended
                    if self.speech_buffer.len() >= self.min_speech_samples {
                        completed.push(SpeechSegment {
                            samples: std::mem::take(&mut self.speech_buffer),
                            start_ms: self.sample_to_ms(self.speech_start_sample),
                            end_ms: self.sample_to_ms(chunk_end_abs),
                        });
                    } else {
                        self.speech_buffer.clear();
                    }
                    self.in_speech = false;
                    self.silent_samples = 0;
                }
            }

            i = chunk_end;
        }

        self.samples_processed += samples.len();
        completed
    }

    /// Flush any in-progress speech (call when session ends).
    pub fn flush(&mut self) -> Option<SpeechSegment> {
        if !self.in_speech || self.speech_buffer.len() < self.min_speech_samples {
            self.speech_buffer.clear();
            self.in_speech = false;
            return None;
        }
        self.in_speech = false;
        Some(SpeechSegment {
            samples: std::mem::take(&mut self.speech_buffer),
            start_ms: self.sample_to_ms(self.speech_start_sample),
            end_ms: self.sample_to_ms(self.samples_processed),
        })
    }
}
