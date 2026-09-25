/*!
 * SOURCE OF TRUTH KEYWORDS: SoundPlayer, play sound cue, chime playback, audio output, fire and forget sound
 * WHAT:  SoundPlayer plays a short mono clip on the default audio output and returns at once.
 * WHY:   Sound cues (start, stop, cancel, error) confirm a take without looking at the pill (01 §7). Playback is an
 *        OS integration, so it sits behind a port: the pipeline renders the clip, an adapter decides how it reaches
 *        the speakers. `play` never waits for the sound to finish or for the device to open, because the session
 *        actor calls it between latency-critical effects; a clip still playing is cut off by the next one, so cues
 *        never pile up. A failure (no output device) is reported for the log only: a missing chime never fails a take.
 * WHERE: Implemented by adapters/sound/win32_play_sound.rs (Win32SoundPlayer) and ports/fakes; used by
 *        pipeline/sound_cues.rs (SoundCues) for the session actor's Cue effects.
 */

use std::sync::Arc;

use crate::types::{PortResult, SoundClip};

/// Short sounds on the default output.
pub trait SoundPlayer: Send + Sync {
    /// Starts playing `clip`, replacing any clip still playing; returns without waiting for it.
    fn play(&self, clip: Arc<SoundClip>) -> PortResult<()>;
}
