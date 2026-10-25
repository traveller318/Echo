/*!
 * SOURCE OF TRUTH KEYWORDS: CpalSoundPlayer, sound cue Linux, cpal output stream, default output device, linear resample, replace playing clip
 * WHAT:  CpalSoundPlayer: SoundPlayer that plays each clip on the default output device through cpal (PulseAudio or
 *        PipeWire's pulse server, else ALSA). A sound thread receives clips; each one replaces the stream of the one
 *        before, is resampled linearly to the device rate and copied to every channel, and its stream is closed
 *        shortly after the clip ends so the output device is not held open between cues.
 * WHY:   Linux has no system "play this WAV" call every desktop ships; cpal is already linked for capture. Cues are
 *        a few hundred milliseconds of mono, so linear interpolation is inaudible next to a real resampler and keeps
 *        the callback to arithmetic: the clip is shared read-only and the position is an atomic, so the real-time
 *        callback never allocates or locks (02 §6.1). A device that cannot open is logged at debug, like a failed
 *        PlaySoundW: a missing chime must never fail a take.
 * WHERE: Built by app/bootstrap on Linux into SoundCues; the Windows build uses win32_play_sound.rs.
 */

use std::{
    sync::{
        Arc, PoisonError,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use cpal::{
    FromSample, SizedSample, Stream, StreamConfig,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};

use crate::{
    ports::SoundPlayer,
    types::{AppError, PortError, PortResult, SoundClip},
};

/// How long a stream stays open after its clip ended (the device's own buffer still has to drain).
const DRAIN: Duration = Duration::from_millis(250);

/// Chimes through a cpal output stream.
pub struct CpalSoundPlayer {
    clips: std::sync::Mutex<Option<mpsc::Sender<Arc<SoundClip>>>>,
    thread: std::sync::Mutex<Option<JoinHandle<()>>>,
}

impl CpalSoundPlayer {
    /// Starts the sound thread.
    pub fn new() -> PortResult<Self> {
        let (clips, received) = mpsc::channel::<Arc<SoundClip>>();
        let thread = thread::Builder::new()
            .name("echo-sound".to_owned())
            .spawn(move || run(&received))
            .map_err(|error| {
                PortError::new(AppError::Internal)
                    .with_detail(format!("the sound thread could not start: {error}"))
            })?;
        Ok(Self {
            clips: std::sync::Mutex::new(Some(clips)),
            thread: std::sync::Mutex::new(Some(thread)),
        })
    }
}

impl SoundPlayer for CpalSoundPlayer {
    fn play(&self, clip: Arc<SoundClip>) -> PortResult<()> {
        let clips = self.clips.lock().unwrap_or_else(PoisonError::into_inner);
        clips
            .as_ref()
            .and_then(|sender| sender.send(clip).ok())
            .ok_or_else(|| {
                PortError::new(AppError::Internal).with_detail("the sound thread has stopped")
            })
    }
}

impl Drop for CpalSoundPlayer {
    fn drop(&mut self) {
        // Closing the channel ends the thread, which drops (stops) the playing stream.
        self.clips
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        let thread = self
            .thread
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(thread) = thread
            && thread.join().is_err()
        {
            tracing::error!("the sound thread panicked");
        }
    }
}

/// The sound thread: plays each clip received, closing its stream once it has drained.
fn run(received: &mpsc::Receiver<Arc<SoundClip>>) {
    let mut playing: Option<Stream> = None;
    let mut wait = None;
    loop {
        let next = match wait {
            Some(timeout) => received.recv_timeout(timeout),
            None => received
                .recv()
                .map_err(|_| mpsc::RecvTimeoutError::Disconnected),
        };
        match next {
            Ok(clip) => {
                // The previous cue stops here, before the new one opens the device.
                playing = None;
                match open(&clip) {
                    Ok(stream) => {
                        playing = Some(stream);
                        wait = Some(Duration::from_millis(clip.duration_ms()) + DRAIN);
                    }
                    Err(detail) => {
                        tracing::debug!(%detail, "a sound cue could not be played (no output device?)");
                        wait = None;
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                playing = None;
                wait = None;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    drop(playing);
}

/// Opens the default output device and starts `clip` on it.
fn open(clip: &Arc<SoundClip>) -> Result<Stream, String> {
    let host = cpal::host_from_id(cpal::HostId::PulseAudio)
        .or_else(|_| cpal::host_from_id(cpal::HostId::Alsa))
        .map_err(|error| error.to_string())?;
    let device = host
        .default_output_device()
        .ok_or_else(|| "no default output device".to_owned())?;
    let supported = device
        .default_output_config()
        .map_err(|error| error.to_string())?;
    let config = supported.config();
    let stream = match supported.sample_format() {
        cpal::SampleFormat::F32 => build::<f32>(&device, &config, clip),
        cpal::SampleFormat::I16 => build::<i16>(&device, &config, clip),
        cpal::SampleFormat::I32 => build::<i32>(&device, &config, clip),
        cpal::SampleFormat::U16 => build::<u16>(&device, &config, clip),
        other => return Err(format!("unsupported output sample format {other:?}")),
    }
    .map_err(|error| error.to_string())?;
    stream.play().map_err(|error| error.to_string())?;
    Ok(stream)
}

fn build<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    clip: &Arc<SoundClip>,
) -> Result<Stream, cpal::Error>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = usize::from(config.channels.max(1));
    let step = f64::from(clip.sample_rate) / f64::from(config.sample_rate.max(1));
    let clip = Arc::clone(clip);
    let frame = AtomicUsize::new(0);
    device.build_output_stream::<T, _, _>(
        *config,
        move |data: &mut [T], _| {
            let mut index = frame.load(Ordering::Relaxed);
            for out in data.chunks_mut(channels) {
                let value = T::from_sample(sample_at(&clip.samples, index as f64 * step));
                out.fill(value);
                index += 1;
            }
            frame.store(index, Ordering::Relaxed);
        },
        |error| tracing::debug!(%error, "sound cue output glitch"),
        None,
    )
}

/// The clip's value at fractional position `at` (linear interpolation; silence past the end).
fn sample_at(samples: &[f32], at: f64) -> f32 {
    let index = at as usize;
    let Some(&first) = samples.get(index) else {
        return 0.0;
    };
    let second = samples.get(index + 1).copied().unwrap_or(0.0);
    let fraction = (at - index as f64) as f32;
    (first + (second - first) * fraction).clamp(-1.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interpolation_is_linear_and_silent_past_the_end() {
        let samples = [0.0, 1.0, -1.0];
        assert!((sample_at(&samples, 0.5) - 0.5).abs() < 1e-6);
        assert!((sample_at(&samples, 1.0) - 1.0).abs() < 1e-6);
        assert!(sample_at(&samples, 1.5).abs() < 1e-6);
        assert_eq!(sample_at(&samples, 3.0), 0.0);
    }

    #[test]
    fn playing_never_fails_even_without_an_output_device() {
        let player = CpalSoundPlayer::new().unwrap();
        player
            .play(Arc::new(SoundClip {
                sample_rate: 16_000,
                samples: vec![0.0; 160],
            }))
            .unwrap();
    }
}
