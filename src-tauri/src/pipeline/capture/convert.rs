/*!
 * SOURCE OF TRUTH KEYWORDS: Converter, downmix, resample to 16 kHz, rubato Fft, pcm16 quantize, pipeline sample format, audio conversion
 * WHAT:  Converter turns interleaved device samples (any rate, any channel count) into 16 kHz mono f32; `downmix`,
 *        `to_pcm16` and `from_pcm16` are the sample-level steps.
 * WHY:   ASR, VAD and the WAV journal all consume one 16 kHz mono stream (05 A2), so conversion happens exactly
 *        once, on the capture worker. Downmix averages channels (a mic reported as stereo is usually two copies,
 *        so averaging keeps its level). Resampling uses rubato's synchronous FFT resampler: device and pipeline
 *        rates are fixed for a take, and it is both fast and high quality. Its start-up delay is trimmed and the
 *        tail flushed on `finish`, so N input frames always become exactly N × 16000 / rate output frames and a
 *        take's duration is exact. 16 kHz input bypasses the resampler. `to_pcm16` / `from_pcm16` define the
 *        journal's 16-bit format; the worker feeds VAD and ASR the round-tripped samples, so a retry from the
 *        journal hears exactly what the first pass heard.
 * WHERE: Owned by the capture worker (pipeline/capture/mod.rs); `from_pcm16` is also used by the journal reader.
 */

use rubato::{Fft, FixedSync, Indexing, Resampler, audioadapter_buffers::direct::InterleavedSlice};

use crate::types::{AppError, CaptureFormat, PIPELINE_SAMPLE_RATE_HZ, PortError, PortResult};

/// Input frames per resampler call (about 21 ms at 48 kHz).
const RESAMPLER_CHUNK: usize = 1024;

/// Full scale of the journal's 16-bit samples; symmetric so +1.0 and -1.0 both round-trip.
const PCM16_SCALE: f32 = i16::MAX as f32;

/// Converts device audio to the pipeline's 16 kHz mono f32.
pub struct Converter {
    channels: usize,
    mono: Vec<f32>,
    resampler: Option<Resampling>,
}

impl Converter {
    /// A converter for streams in `format`; fails with `AudioDevice` for a format with no rate or no channels.
    pub fn new(format: CaptureFormat) -> PortResult<Self> {
        if format.sample_rate == 0 || format.channels == 0 {
            return Err(PortError::new(AppError::AudioDevice).with_detail(format!(
                "the device reports an unusable format ({} Hz, {} channels)",
                format.sample_rate, format.channels
            )));
        }
        let resampler = if format.sample_rate == PIPELINE_SAMPLE_RATE_HZ {
            None
        } else {
            Some(Resampling::new(format.sample_rate)?)
        };
        Ok(Self {
            channels: usize::from(format.channels),
            mono: Vec::new(),
            resampler,
        })
    }

    /// Appends the 16 kHz mono version of `interleaved` (whole frames) to `out`.
    pub fn push(&mut self, interleaved: &[f32], out: &mut Vec<f32>) -> PortResult<()> {
        match &mut self.resampler {
            None => downmix(interleaved, self.channels, out),
            Some(resampling) => {
                self.mono.clear();
                downmix(interleaved, self.channels, &mut self.mono);
                resampling.push(&self.mono, out)?;
            }
        }
        Ok(())
    }

    /// Appends whatever the resampler still holds, so the output length matches the input exactly.
    pub fn finish(&mut self, out: &mut Vec<f32>) -> PortResult<()> {
        match &mut self.resampler {
            None => Ok(()),
            Some(resampling) => resampling.finish(out),
        }
    }
}

/// Appends the average of each frame's channels to `out`; a trailing partial frame is ignored.
pub fn downmix(interleaved: &[f32], channels: usize, out: &mut Vec<f32>) {
    if channels <= 1 {
        out.extend_from_slice(interleaved);
        return;
    }
    // Channel counts are at most a few dozen, so the conversion is exact.
    let scale = 1.0 / channels as f32;
    out.extend(
        interleaved
            .chunks_exact(channels)
            .map(|frame| frame.iter().sum::<f32>() * scale),
    );
}

/// The journal's 16-bit value of `sample` (clamped to [-1, 1], rounded).
pub fn to_pcm16(sample: f32) -> i16 {
    let scaled = (sample.clamp(-1.0, 1.0) * PCM16_SCALE).round();
    // In range after the clamp; `as` saturates and maps NaN to 0.
    scaled as i16
}

/// The f32 value of a journal sample.
pub fn from_pcm16(sample: i16) -> f32 {
    (f32::from(sample) / PCM16_SCALE).max(-1.0)
}

/**
 * SOURCE OF TRUTH KEYWORDS: Resampling, rubato Fft, resampler delay trim, resampler tail flush, exact output length
 * WHAT:  A mono FFT resampler from the device rate to 16 kHz with its own input queue: whole chunks are resampled
 *        as they fill, the start-up delay is dropped, and `finish` flushes the tail to the exact expected length.
 * WHY:   The FFT resampler takes fixed input chunks, while callbacks deliver any count; queuing here keeps the
 *        worker simple. Output buffers are allocated once, so steady-state processing does not allocate.
 * WHERE: Converter above.
 */
struct Resampling {
    fft: Fft<f32>,
    input_rate: u32,
    pending: Vec<f32>,
    output: Vec<f32>,
    silence: Vec<f32>,
    delay_left: usize,
    frames_in: u64,
    frames_out: u64,
}

impl Resampling {
    fn new(input_rate: u32) -> PortResult<Self> {
        let fft = Fft::<f32>::new(
            input_rate as usize,
            PIPELINE_SAMPLE_RATE_HZ as usize,
            RESAMPLER_CHUNK,
            1,
            FixedSync::Input,
        )
        .map_err(|error| {
            PortError::new(AppError::AudioDevice)
                .with_detail(format!("no resampler for {input_rate} Hz: {error}"))
        })?;
        let delay_left = fft.output_delay();
        let output = vec![0.0; fft.output_frames_max()];
        let silence = vec![0.0; fft.input_frames_max()];
        Ok(Self {
            fft,
            input_rate,
            pending: Vec::with_capacity(RESAMPLER_CHUNK * 2),
            output,
            silence,
            delay_left,
            frames_in: 0,
            frames_out: 0,
        })
    }

    fn push(&mut self, mono: &[f32], out: &mut Vec<f32>) -> PortResult<()> {
        self.pending.extend_from_slice(mono);
        self.frames_in += mono.len() as u64;
        let mut offset = 0;
        while self.pending.len() - offset >= self.fft.input_frames_next() {
            let consumed = self.process(Source::Pending { offset }, out)?;
            offset += consumed;
        }
        self.pending.drain(..offset);
        Ok(())
    }

    fn finish(&mut self, out: &mut Vec<f32>) -> PortResult<()> {
        let expected =
            self.frames_in * u64::from(PIPELINE_SAMPLE_RATE_HZ) / u64::from(self.input_rate);
        let before = out.len();
        if !self.pending.is_empty() {
            self.process(Source::Tail, out)?;
            self.pending.clear();
        }
        // Silence pushes the delayed tail out; each call yields about a chunk, so this ends after a few calls.
        while self.frames_out < expected {
            let produced_before = self.frames_out;
            self.process(Source::Silence, out)?;
            if self.frames_out == produced_before {
                break;
            }
        }
        let excess = usize::try_from(self.frames_out.saturating_sub(expected)).unwrap_or(0);
        let appended = out.len() - before;
        out.truncate(out.len() - excess.min(appended));
        self.frames_out = self.frames_out.min(expected);
        Ok(())
    }

    /// Runs one resampler call and appends its fresh output; returns the input frames it consumed.
    fn process(&mut self, source: Source, out: &mut Vec<f32>) -> PortResult<usize> {
        let needed = self.fft.input_frames_next();
        let (buffer, indexing) = match source {
            Source::Pending { offset } => (&self.pending[..], Indexing::new().input_offset(offset)),
            Source::Tail => (
                &self.pending[..],
                Indexing::new().partial_len(self.pending.len()),
            ),
            Source::Silence => (&self.silence[..], Indexing::new()),
        };
        let frames = buffer.len();
        let input = InterleavedSlice::new(buffer, 1, frames).map_err(resample_failure)?;
        let output_frames = self.output.len();
        let mut output = InterleavedSlice::new_mut(&mut self.output[..], 1, output_frames)
            .map_err(resample_failure)?;
        let (_, produced) = self
            .fft
            .process_into_buffer(&input, &mut output, Some(&indexing))
            .map_err(resample_failure)?;
        let skipped = produced.min(self.delay_left);
        self.delay_left -= skipped;
        out.extend_from_slice(&self.output[skipped..produced]);
        self.frames_out += (produced - skipped) as u64;
        Ok(needed)
    }
}

/// Where one resampler call reads from.
#[derive(Clone, Copy)]
enum Source {
    /// A whole chunk of queued input starting at `offset`.
    Pending { offset: usize },
    /// The queued remainder, shorter than a chunk (padded with silence by rubato).
    Tail,
    /// A chunk of silence that flushes the delay line.
    Silence,
}

fn resample_failure(error: impl std::fmt::Display) -> PortError {
    PortError::new(AppError::Internal).with_detail(format!("resampling failed: {error}"))
}

#[cfg(test)]
mod tests {
    use std::f32::consts::TAU;

    use super::*;

    fn sine(rate: u32, channels: u16, seconds: f32, hz: f32, amplitude: f32) -> Vec<f32> {
        let frames = (rate as f32 * seconds) as usize;
        (0..frames)
            .flat_map(|frame| {
                let value = amplitude * (TAU * hz * frame as f32 / rate as f32).sin();
                std::iter::repeat_n(value, usize::from(channels))
            })
            .collect()
    }

    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|sample| sample * sample).sum::<f32>() / samples.len() as f32).sqrt()
    }

    fn rising_zero_crossings(samples: &[f32]) -> usize {
        samples
            .windows(2)
            .filter(|pair| pair[0] < 0.0 && pair[1] >= 0.0)
            .count()
    }

    /// Feeds `input` in uneven slices, the way device callbacks arrive.
    fn convert(format: CaptureFormat, input: &[f32]) -> Vec<f32> {
        let mut converter = Converter::new(format).unwrap();
        let mut out = Vec::new();
        let frame = usize::from(format.channels);
        for chunk in input.chunks(441 * frame) {
            converter.push(chunk, &mut out).unwrap();
        }
        converter.finish(&mut out).unwrap();
        out
    }

    #[test]
    fn downmix_averages_the_channels_of_each_frame() {
        let mut out = Vec::new();
        downmix(&[1.0, 0.0, 0.5, 0.5, -1.0, 1.0], 2, &mut out);
        assert_eq!(out, [0.5, 0.5, 0.0]);
        out.clear();
        downmix(&[0.3, 0.3, 0.3, 0.9], 4, &mut out);
        assert!((out[0] - 0.45).abs() < 1e-6);
        out.clear();
        downmix(&[0.1, 0.2], 1, &mut out);
        assert_eq!(out, [0.1, 0.2]);
    }

    #[test]
    fn stereo_48k_becomes_exactly_one_second_of_16k_mono_with_the_same_tone() {
        let input = sine(48_000, 2, 1.0, 1_000.0, 0.5);
        let out = convert(
            CaptureFormat {
                sample_rate: 48_000,
                channels: 2,
            },
            &input,
        );
        assert_eq!(out.len(), 16_000);
        // Skip the edges, where the filter sees the start and end of the tone.
        let steady = &out[800..15_200];
        assert!(
            (rms(steady) - 0.5 / 2_f32.sqrt()).abs() < 0.01,
            "{}",
            rms(steady)
        );
        let cycles = rising_zero_crossings(steady);
        assert!((898..=902).contains(&cycles), "{cycles} cycles in 0.9 s");
    }

    #[test]
    fn any_device_rate_keeps_the_duration_exact() {
        for rate in [8_000, 22_050, 44_100, 96_000] {
            let input = sine(rate, 1, 0.75, 440.0, 0.25);
            let out = convert(
                CaptureFormat {
                    sample_rate: rate,
                    channels: 1,
                },
                &input,
            );
            let expected = input.len() as u64 * 16_000 / u64::from(rate);
            assert_eq!(out.len() as u64, expected, "{rate} Hz");
        }
    }

    #[test]
    fn pipeline_rate_mono_passes_through_untouched() {
        let input = sine(16_000, 1, 0.1, 300.0, 0.4);
        let out = convert(
            CaptureFormat {
                sample_rate: 16_000,
                channels: 1,
            },
            &input,
        );
        assert_eq!(out, input);
    }

    #[test]
    fn unusable_formats_are_refused() {
        for format in [
            CaptureFormat {
                sample_rate: 0,
                channels: 1,
            },
            CaptureFormat {
                sample_rate: 48_000,
                channels: 0,
            },
        ] {
            assert_eq!(
                Converter::new(format).err().map(PortError::into_app_error),
                Some(AppError::AudioDevice)
            );
        }
    }

    #[test]
    fn pcm16_round_trips_within_one_step_and_clamps() {
        assert_eq!(to_pcm16(1.0), i16::MAX);
        assert_eq!(to_pcm16(-1.0), -i16::MAX);
        assert_eq!(to_pcm16(2.0), i16::MAX);
        assert_eq!(to_pcm16(f32::NAN), 0);
        assert_eq!(from_pcm16(i16::MIN), -1.0);
        for sample in [-0.73_f32, -0.001, 0.0, 0.25, 0.999] {
            assert!((from_pcm16(to_pcm16(sample)) - sample).abs() <= 1.0 / PCM16_SCALE);
        }
    }
}
