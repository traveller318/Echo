/*!
 * SOURCE OF TRUTH KEYWORDS: Win32SoundPlayer, PlaySoundW, SND_MEMORY, SND_ASYNC, in-memory WAV, sound thread, wav encoding
 * WHAT:  Win32SoundPlayer: SoundPlayer on WinMM's PlaySoundW. `play` hands the clip to a thread of its own, which
 *        encodes it as a 16-bit PCM WAV image in memory and starts it asynchronously on the default output.
 * WHY:   PlaySoundW plays an in-memory WAV with no device setup, mixes with other apps and follows Echo's volume in
 *        the Windows mixer, which is all a sub-second chime needs. With SND_ASYNC | SND_MEMORY the image must stay
 *        valid until the sound ends, so the thread keeps the playing image and only lets go of it after the next
 *        PlaySoundW has replaced it (which stops the old one) or after stopping playback on shutdown. The first call
 *        in a process loads WinMM and the audio engine (tens of ms), and `play` must return at once (ports/sound.rs),
 *        so the call never runs on the caller's thread. SND_NODEFAULT keeps Windows from playing its own "ding" when
 *        a clip cannot be played; the failure is logged instead. The WAV is built as 16-bit words so the buffer is
 *        naturally aligned for the wide-string pointer PlaySoundW takes.
 * WHERE: Built by app/bootstrap into SoundCues (pipeline/sound_cues.rs); used only through `dyn SoundPlayer`.
 */

use std::{
    io::Cursor,
    sync::{Arc, Mutex, PoisonError, mpsc},
    thread::{self, JoinHandle},
};

use windows::{
    Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_FLAGS, SND_MEMORY, SND_NODEFAULT},
    core::PCWSTR,
};

use crate::{
    ports::SoundPlayer,
    types::{AppError, PortError, PortResult, SoundClip},
};

/// Chimes through WinMM's PlaySoundW.
pub struct Win32SoundPlayer {
    clips: Mutex<Option<mpsc::Sender<Arc<SoundClip>>>>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl Win32SoundPlayer {
    /// Starts the sound thread.
    pub fn new() -> PortResult<Self> {
        let (clips, received) = mpsc::channel::<Arc<SoundClip>>();
        let thread = thread::Builder::new()
            .name("echo-sound".to_owned())
            .spawn(move || {
                // The image PlaySoundW is reading; kept until the next one replaces it.
                let mut playing: Option<Vec<u16>> = None;
                while let Ok(clip) = received.recv() {
                    match wav_image(&clip) {
                        Ok(image) => {
                            // SAFETY: `image` is a complete WAV file in memory; it is moved into `playing` right after
                            // and kept alive until another PlaySoundW call has stopped it.
                            let started = unsafe {
                                PlaySoundW(
                                    PCWSTR(image.as_ptr()),
                                    None,
                                    SND_MEMORY | SND_ASYNC | SND_NODEFAULT,
                                )
                            };
                            if !started.as_bool() {
                                tracing::debug!(
                                    "a sound cue could not be played (no output device?)"
                                );
                            }
                            playing = Some(image);
                        }
                        Err(detail) => tracing::warn!(%detail, "a sound cue could not be encoded"),
                    }
                }
                if playing.take().is_some() {
                    // SAFETY: a null sound stops whatever this process is playing, so the image can be freed.
                    let _ = unsafe { PlaySoundW(PCWSTR::null(), None, SND_FLAGS(0)) };
                }
            })
            .map_err(|error| {
                PortError::new(AppError::Internal)
                    .with_detail(format!("the sound thread could not start: {error}"))
            })?;
        Ok(Self {
            clips: Mutex::new(Some(clips)),
            thread: Mutex::new(Some(thread)),
        })
    }
}

impl SoundPlayer for Win32SoundPlayer {
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

impl Drop for Win32SoundPlayer {
    fn drop(&mut self) {
        // Closing the channel ends the thread, which stops playback before freeing the image.
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

/**
 * SOURCE OF TRUTH KEYWORDS: wav_image, encode WAV in memory, 16-bit PCM, f32 to i16
 * WHAT:  The clip as a mono 16-bit PCM WAV file in memory, as 16-bit words (little-endian bytes, like the file).
 * WHY:   16-bit PCM is the WAV format every Windows player accepts; samples are clamped to [-1, 1] first so a
 *        rounding overshoot cannot wrap around into a click. A WAV header is 44 bytes and each sample 2, so the byte
 *        length is always even and fits 16-bit words exactly.
 * WHERE: The sound thread above; tests.
 */
fn wav_image(clip: &SoundClip) -> Result<Vec<u16>, String> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: clip.sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut bytes = Cursor::new(Vec::with_capacity(44 + clip.samples.len() * 2));
    {
        let mut writer =
            hound::WavWriter::new(&mut bytes, spec).map_err(|error| error.to_string())?;
        for sample in &clip.samples {
            let pcm = (sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)).round() as i16;
            writer
                .write_sample(pcm)
                .map_err(|error| error.to_string())?;
        }
        writer.finalize().map_err(|error| error.to_string())?;
    }
    let bytes = bytes.into_inner();
    let (words, rest) = bytes.as_chunks::<2>();
    if !rest.is_empty() {
        return Err(format!(
            "a WAV image of {} bytes is not whole words",
            bytes.len()
        ));
    }
    Ok(words.iter().map(|pair| u16::from_le_bytes(*pair)).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clip(samples: Vec<f32>) -> Arc<SoundClip> {
        Arc::new(SoundClip {
            sample_rate: 44_100,
            samples,
        })
    }

    #[test]
    fn a_clip_encodes_as_a_readable_16_bit_wav() {
        let image = wav_image(&clip(vec![0.0, 0.5, -1.0, 2.0])).unwrap();
        let bytes: Vec<u8> = image.iter().flat_map(|word| word.to_le_bytes()).collect();
        let reader = hound::WavReader::new(Cursor::new(bytes)).unwrap();
        let spec = reader.spec();
        assert_eq!(
            (spec.channels, spec.sample_rate, spec.bits_per_sample),
            (1, 44_100, 16)
        );
        let samples: Vec<i16> = reader.into_samples().map(Result::unwrap).collect();
        assert_eq!(
            samples,
            [0, 16_384, -i16::MAX, i16::MAX],
            "out-of-range samples are clamped"
        );
    }

    #[test]
    fn silence_plays_and_the_player_shuts_down_cleanly() {
        let player = Win32SoundPlayer::new().unwrap();
        player.play(clip(vec![0.0; 441])).unwrap();
        player.play(clip(vec![0.0; 441])).unwrap();
        drop(player);
    }
}
