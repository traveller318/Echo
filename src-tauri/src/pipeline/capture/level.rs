/*!
 * SOURCE OF TRUTH KEYWORDS: LevelMeter, RMS level, AudioLevel rate, 30 Hz, level window, peak rms, mean rms, mic verdict thresholds
 * WHAT:  LevelMeter turns the 16 kHz stream into one RMS value per LEVEL_WINDOW_SAMPLES window (the AudioLevel
 *        event) and keeps the take's peak and mean RMS; `verdict` reads a microphone check from those levels.
 * WHY:   AudioLevel is capped at 30 Hz (02 §4.4). Counting samples instead of reading a clock keeps the rate exact,
 *        deterministic in tests, and free of timers; a window of 534 samples is 33.4 ms, so at most 30 values per
 *        second of audio. No samples arrive while capture is paused, so no level is sent outside recording. The
 *        verdict thresholds are pipeline policy: exact digital silence means the device delivers nothing (muted or
 *        blocked by Windows privacy, 05 W13), which is a different fix from a mic that is merely quiet.
 * WHERE: Owned by the capture worker (pipeline/capture/mod.rs); `verdict` is used by pipeline/capture/mic_check.rs.
 */

use crate::types::{MicVerdict, PIPELINE_SAMPLE_RATE_HZ};

/// Samples per level window: the smallest window that keeps AudioLevel at or below 30 Hz.
pub const LEVEL_WINDOW_SAMPLES: usize = (PIPELINE_SAMPLE_RATE_HZ as usize).div_ceil(30);

/// Peak RMS below this is digital silence (about -80 dBFS).
const NO_SIGNAL_RMS: f32 = 1e-4;

/// Peak RMS below this is too quiet for reliable recognition (-40 dBFS).
const TOO_QUIET_RMS: f32 = 0.01;

/// Peak RMS above this means speech is clipping (-6 dBFS).
const TOO_LOUD_RMS: f32 = 0.5;

/// Windowed and whole-take RMS of the 16 kHz stream.
#[derive(Debug, Default)]
pub struct LevelMeter {
    window_sum: f64,
    window_len: usize,
    total_sum: f64,
    total_len: u64,
    peak_rms: f32,
}

impl LevelMeter {
    /// Adds `samples`; calls `on_level` with the RMS of every window that completes.
    pub fn push(&mut self, samples: &[f32], mut on_level: impl FnMut(f32)) {
        for &sample in samples {
            let square = f64::from(sample) * f64::from(sample);
            self.window_sum += square;
            self.total_sum += square;
            self.window_len += 1;
            if self.window_len == LEVEL_WINDOW_SAMPLES {
                let level = rms(self.window_sum, LEVEL_WINDOW_SAMPLES as u64);
                self.peak_rms = self.peak_rms.max(level);
                self.window_sum = 0.0;
                self.window_len = 0;
                on_level(level);
            }
        }
        self.total_len += samples.len() as u64;
    }

    /// Loudest completed window; a take shorter than one window counts its partial window.
    pub fn peak_rms(&self) -> f32 {
        if self.window_len > 0 {
            self.peak_rms
                .max(rms(self.window_sum, self.window_len as u64))
        } else {
            self.peak_rms
        }
    }

    /// RMS over everything pushed.
    pub fn mean_rms(&self) -> f32 {
        rms(self.total_sum, self.total_len)
    }
}

/// What a microphone check's peak level means.
pub fn verdict(peak_rms: f32) -> MicVerdict {
    if peak_rms < NO_SIGNAL_RMS {
        MicVerdict::NoSignal
    } else if peak_rms < TOO_QUIET_RMS {
        MicVerdict::TooQuiet
    } else if peak_rms > TOO_LOUD_RMS {
        MicVerdict::TooLoud
    } else {
        MicVerdict::Good
    }
}

fn rms(sum_of_squares: f64, count: u64) -> f32 {
    if count == 0 {
        return 0.0;
    }
    // Samples are in [-1, 1], so the result is in [0, 1] and fits f32.
    ((sum_of_squares / count as f64).sqrt() as f32).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_level_is_sent_at_most_thirty_times_per_second() {
        let mut meter = LevelMeter::default();
        let mut levels = Vec::new();
        for chunk in vec![0.5_f32; 16_000].chunks(700) {
            meter.push(chunk, |level| levels.push(level));
        }
        assert_eq!(levels.len(), 29);
        const { assert!(LEVEL_WINDOW_SAMPLES * 30 >= 16_000) };
        assert!(levels.iter().all(|level| (level - 0.5).abs() < 1e-6));
    }

    #[test]
    fn peak_and_mean_describe_the_whole_take() {
        let mut meter = LevelMeter::default();
        meter.push(&vec![0.0; LEVEL_WINDOW_SAMPLES], |_| {});
        meter.push(&vec![0.8; LEVEL_WINDOW_SAMPLES], |_| {});
        assert!((meter.peak_rms() - 0.8).abs() < 1e-6);
        assert!((meter.mean_rms() - (0.32_f32).sqrt()).abs() < 1e-4);

        let mut short = LevelMeter::default();
        short.push(&[0.2; 100], |_| panic!("no full window"));
        assert!((short.peak_rms() - 0.2).abs() < 1e-6);
        assert_eq!(LevelMeter::default().mean_rms(), 0.0);
    }

    #[test]
    fn verdicts_separate_silence_quiet_good_and_clipping() {
        assert_eq!(verdict(0.0), MicVerdict::NoSignal);
        assert_eq!(verdict(0.005), MicVerdict::TooQuiet);
        assert_eq!(verdict(0.1), MicVerdict::Good);
        assert_eq!(verdict(0.7), MicVerdict::TooLoud);
    }
}
