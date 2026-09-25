/*!
 * SOURCE OF TRUTH KEYWORDS: sound cue types, Tone, CueSound, SoundClip, synthesized chime, mono PCM clip, sound cue spec
 * WHAT:  The data shapes of sound cues: a Tone (a sine at a pitch for a duration, or a silent gap), a CueSound (the
 *        tones one SessionCue plays and how loud), and a SoundClip (mono f32 samples ready for a SoundPlayer).
 * WHY:   Cues are synthesized in code from registry entries instead of shipped as audio files, so they are
 *        license-clean by construction, cost no installer bytes and never need a download (05 decision log). A cue is
 *        a list of tones so a new or different chime is a registry edit, not new code; the rendered clip is plain
 *        samples, so a player backend (PlaySound today, a WASAPI render stream or user-picked WAV files later)
 *        decides its own encoding.
 * WHERE: CueSound entries live in registry/sounds.rs; pipeline/sound_cues.rs renders them into SoundClips; the
 *        SoundPlayer port (ports/sound.rs) plays a SoundClip.
 */

use super::SessionCue;

/// One step of a cue: a sine at `hz` for `ms` milliseconds; `hz` 0 is a silent gap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Tone {
    pub hz: u32,
    pub ms: u32,
}

impl Tone {
    /// A sine at `hz` for `ms`.
    pub const fn note(hz: u32, ms: u32) -> Self {
        Self { hz, ms }
    }

    /// Silence for `ms`.
    pub const fn rest(ms: u32) -> Self {
        Self { hz: 0, ms }
    }
}

/// How one session cue sounds: its tones in order, at `gain_percent` of full scale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CueSound {
    pub cue: SessionCue,
    pub tones: &'static [Tone],
    /// Peak amplitude as a percentage of full scale (1–100).
    pub gain_percent: u8,
}

impl CueSound {
    /// Total length of the cue, in ms.
    pub fn duration_ms(&self) -> u32 {
        self.tones.iter().map(|tone| tone.ms).sum()
    }
}

/// Mono audio ready to play: f32 samples in [-1, 1] at `sample_rate` Hz.
#[derive(Debug, Clone, PartialEq)]
pub struct SoundClip {
    pub sample_rate: u32,
    pub samples: Vec<f32>,
}

impl SoundClip {
    /// Length of the clip, in ms.
    pub fn duration_ms(&self) -> u64 {
        if self.sample_rate == 0 {
            return 0;
        }
        self.samples.len() as u64 * 1000 / u64::from(self.sample_rate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_add_up() {
        const TONES: &[Tone] = &[Tone::note(660, 60), Tone::rest(20), Tone::note(880, 80)];
        let cue = CueSound {
            cue: SessionCue::Start,
            tones: TONES,
            gain_percent: 20,
        };
        assert_eq!(cue.duration_ms(), 160);
        let clip = SoundClip {
            sample_rate: 1_000,
            samples: vec![0.0; 250],
        };
        assert_eq!(clip.duration_ms(), 250);
        assert_eq!(
            SoundClip {
                sample_rate: 0,
                samples: vec![0.0; 4],
            }
            .duration_ms(),
            0
        );
    }
}
