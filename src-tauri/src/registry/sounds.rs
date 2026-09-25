/*!
 * SOURCE OF TRUTH KEYWORDS: sound cue registry, CUE_SOUNDS, start chime, stop chime, cancel chime, error chime, sound_for, synthesized cues
 * WHAT:  CUE_SOUNDS: how every SessionCue sounds, as a short list of sine tones and a gain; `sound_for(cue)` finds
 *        the entry.
 * WHY:   A cue's sound is data, so changing or adding one is an entry here (root CLAUDE.md §3), never a match in the
 *        pipeline. The tones are synthesized at startup (pipeline/sound_cues.rs): nothing is downloaded, nothing
 *        licensed, nothing added to the installer (05 decision log). The shapes follow a common convention: start
 *        rises, stop falls, cancel falls lower and softer, an error is two low pulses. Every cue is under 250 ms and
 *        quiet (at most 20% of full scale) because the start chime plays while the microphone listens: short and soft
 *        keeps it out of what the detector calls speech.
 * WHERE: pipeline/sound_cues.rs renders every entry once, when the session actor is built.
 */

use crate::types::{CueSound, SessionCue, Tone};

/// Every cue's sound, one entry per SessionCue.
pub const CUE_SOUNDS: &[CueSound] = &[
    CueSound {
        cue: SessionCue::Start,
        tones: &[Tone::note(659, 70), Tone::note(880, 90)],
        gain_percent: 18,
    },
    CueSound {
        cue: SessionCue::Stop,
        tones: &[Tone::note(880, 70), Tone::note(659, 90)],
        gain_percent: 18,
    },
    CueSound {
        cue: SessionCue::Cancel,
        tones: &[Tone::note(392, 80), Tone::rest(20), Tone::note(294, 110)],
        gain_percent: 15,
    },
    CueSound {
        cue: SessionCue::Error,
        tones: &[Tone::note(330, 80), Tone::rest(50), Tone::note(330, 80)],
        gain_percent: 18,
    },
];

/// The sound of `cue`; None only if an entry is missing, which the tests below rule out.
pub fn sound_for(cue: SessionCue) -> Option<&'static CueSound> {
    CUE_SOUNDS.iter().find(|sound| sound.cue == cue)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn every_cue_has_exactly_one_sound() {
        let cues: HashSet<SessionCue> = CUE_SOUNDS.iter().map(|sound| sound.cue).collect();
        assert_eq!(cues.len(), CUE_SOUNDS.len(), "a cue is listed twice");
        for cue in SessionCue::ALL {
            assert_eq!(sound_for(cue).map(|sound| sound.cue), Some(cue));
        }
    }

    #[test]
    fn cues_are_short_quiet_and_audible() {
        for sound in CUE_SOUNDS {
            assert!(
                (1..=20).contains(&sound.gain_percent),
                "{:?} is too loud or silent",
                sound.cue
            );
            assert!(
                (60..=250).contains(&sound.duration_ms()),
                "{:?} lasts {} ms",
                sound.cue,
                sound.duration_ms()
            );
            assert!(
                sound
                    .tones
                    .iter()
                    .all(|tone| tone.hz == 0 || (200..=4_000).contains(&tone.hz)),
                "{:?} has a tone outside the comfortable range",
                sound.cue
            );
            assert!(sound.tones.iter().any(|tone| tone.hz > 0));
        }
    }
}
