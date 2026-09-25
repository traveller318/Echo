/*!
 * SOURCE OF TRUTH KEYWORDS: SoundCues, render cue, synthesize tone, sine chime, fade envelope, play cue, general.sound_cues, CUE_SAMPLE_RATE_HZ
 * WHAT:  `render(sound)` turns a registry CueSound into a SoundClip (sine tones with short fades, silent rests).
 *        SoundCues renders every registry cue once and plays one through the SoundPlayer port when
 *        `general.sound_cues` is on in the settings passed in.
 * WHY:   Cues are synthesized in code, so they are license-clean and cost nothing to ship (05 decision log);
 *        rendering once at build time keeps `play` to one port call on the actor's path. Each tone starts and ends
 *        with a FADE_MS raised-cosine ramp, because a sine cut at a non-zero sample clicks. The setting is read at
 *        the moment of the cue, so turning cues off in Settings silences the very next one. A player failure (no
 *        output device) is logged at debug level and never touches the take: a chime is feedback, not function.
 * WHERE: Built by app/bootstrap (and actor tests) into SessionConfig; `play` is called by the session runner for
 *        every SessionEffect::Cue.
 */

use std::{f32::consts::TAU, sync::Arc};

use crate::{
    ports::SoundPlayer,
    registry,
    types::{CueSound, SessionCue, SettingsSnapshot, SoundClip},
};

/// Sample rate cues are rendered at: every Windows output device plays it without an odd resample.
pub const CUE_SAMPLE_RATE_HZ: u32 = 44_100;

/// Fade in and out of every tone, in ms.
const FADE_MS: u32 = 5;

/// Samples in `ms` milliseconds at CUE_SAMPLE_RATE_HZ.
fn samples_in(ms: u32) -> usize {
    usize::try_from(u64::from(ms) * u64::from(CUE_SAMPLE_RATE_HZ) / 1000).unwrap_or(0)
}

/**
 * SOURCE OF TRUTH KEYWORDS: render cue, tone synthesis, raised cosine fade, gain
 * WHAT:  The mono clip of `sound`: each tone in order, a sine at its pitch with a fade at both ends, or silence for a
 *        rest, scaled to the cue's gain.
 * WHY:   Pure, so the waveform (length, peak, clean edges) is tested without a speaker.
 * WHERE: SoundCues::new.
 */
pub fn render(sound: &CueSound) -> SoundClip {
    let gain = f32::from(sound.gain_percent.min(100)) / 100.0;
    let rate = CUE_SAMPLE_RATE_HZ as f32;
    let mut samples = Vec::with_capacity(samples_in(sound.duration_ms()));
    for tone in sound.tones {
        let length = samples_in(tone.ms);
        if tone.hz == 0 {
            samples.resize(samples.len() + length, 0.0);
            continue;
        }
        let fade = samples_in(FADE_MS).clamp(1, (length / 2).max(1));
        let step = TAU * tone.hz as f32 / rate;
        for index in 0..length {
            let from_edge = index.min(length - 1 - index);
            let envelope = if from_edge < fade {
                // Raised cosine from 0 at the edge to 1 after `fade` samples.
                0.5 - 0.5 * (std::f32::consts::PI * from_edge as f32 / fade as f32).cos()
            } else {
                1.0
            };
            samples.push(gain * envelope * (step * index as f32).sin());
        }
    }
    SoundClip {
        sample_rate: CUE_SAMPLE_RATE_HZ,
        samples,
    }
}

/// The rendered cues and the player they go to; clones share both.
#[derive(Clone)]
pub struct SoundCues {
    player: Arc<dyn SoundPlayer>,
    clips: Arc<[(SessionCue, Arc<SoundClip>)]>,
}

impl SoundCues {
    /// Renders every registry cue for `player`.
    pub fn new(player: Arc<dyn SoundPlayer>) -> Self {
        let clips = registry::sounds::CUE_SOUNDS
            .iter()
            .map(|sound| (sound.cue, Arc::new(render(sound))))
            .collect();
        Self { player, clips }
    }

    /// Plays `cue` when `general.sound_cues` is on in `settings`; returns at once.
    pub fn play(&self, cue: SessionCue, settings: &SettingsSnapshot) {
        if !registry::settings::sound_cues(settings) {
            return;
        }
        let Some((_, clip)) = self.clips.iter().find(|(known, _)| *known == cue) else {
            tracing::warn!(?cue, "this sound cue has no registry sound");
            return;
        };
        if let Err(error) = self.player.play(Arc::clone(clip)) {
            tracing::debug!(
                ?cue,
                detail = error.detail(),
                "a sound cue could not be played"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ports::fakes::FakeSoundPlayer,
        registry::settings::{self, keys},
        types::{AppError, SettingValue, Tone},
    };

    const TWO_NOTES: CueSound = CueSound {
        cue: SessionCue::Start,
        tones: &[Tone::note(660, 40), Tone::rest(10), Tone::note(880, 40)],
        gain_percent: 20,
    };

    #[test]
    fn a_rendered_cue_has_the_length_peak_and_clean_edges_of_its_tones() {
        let clip = render(&TWO_NOTES);
        assert_eq!(clip.sample_rate, CUE_SAMPLE_RATE_HZ);
        assert_eq!(clip.samples.len(), samples_in(90));
        let peak = clip
            .samples
            .iter()
            .fold(0.0_f32, |peak, s| peak.max(s.abs()));
        assert!(peak <= 0.2 + f32::EPSILON && peak > 0.15, "peak {peak}");
        // Every tone starts and ends near zero, so nothing clicks.
        let first = samples_in(40);
        for edge in [0, first - 1, first + samples_in(10), clip.samples.len() - 1] {
            assert!(
                clip.samples[edge].abs() < 0.01,
                "sample {edge} is {}",
                clip.samples[edge]
            );
        }
        // The rest between the notes is silent.
        assert!(
            clip.samples[first..first + samples_in(10)]
                .iter()
                .all(|sample| *sample == 0.0)
        );
    }

    #[test]
    fn every_registry_cue_renders() {
        for sound in registry::sounds::CUE_SOUNDS {
            let clip = render(sound);
            assert_eq!(clip.duration_ms(), u64::from(sound.duration_ms()));
            assert!(clip.samples.iter().all(|sample| sample.abs() <= 1.0));
        }
    }

    #[test]
    fn cues_play_only_while_sound_cues_are_on_and_failures_are_swallowed() {
        let player = Arc::new(FakeSoundPlayer::default());
        let cues = SoundCues::new(Arc::clone(&player) as _);
        let on = settings::defaults();
        cues.play(SessionCue::Start, &on);
        cues.play(SessionCue::Stop, &on);
        let off = settings::resolve([(keys::SOUND_CUES, SettingValue::Bool(false))]);
        cues.play(SessionCue::Cancel, &off);
        player.fail_next(AppError::AudioDevice.into());
        cues.play(SessionCue::Error, &on);

        let expected: Vec<Arc<SoundClip>> = [SessionCue::Start, SessionCue::Stop]
            .into_iter()
            .map(|cue| Arc::new(render(registry::sounds::sound_for(cue).unwrap())))
            .collect();
        assert_eq!(player.played(), expected);
    }
}
