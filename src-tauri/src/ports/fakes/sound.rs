/*!
 * SOURCE OF TRUTH KEYWORDS: FakeSoundPlayer, fake sound cues, recorded clips, sound failure test
 * WHAT:  FakeSoundPlayer: a SoundPlayer that records every clip it was asked to play and can fail the next one.
 * WHY:   Session tests assert which cues a take plays and in which order, that sound cues off plays nothing, and
 *        that a player failure never touches the take.
 * WHERE: pipeline/sound_cues.rs and session actor tests.
 */

use std::sync::{Arc, Mutex};

use super::lock;
use crate::{
    ports::SoundPlayer,
    types::{PortError, PortResult, SoundClip},
};

#[derive(Default)]
struct PlayerLog {
    played: Vec<Arc<SoundClip>>,
    next_error: Option<PortError>,
}

/// A recording sound player.
#[derive(Default)]
pub struct FakeSoundPlayer {
    log: Mutex<PlayerLog>,
}

impl FakeSoundPlayer {
    pub fn fail_next(&self, error: PortError) {
        lock(&self.log).next_error = Some(error);
    }

    /// Every clip played, in order.
    pub fn played(&self) -> Vec<Arc<SoundClip>> {
        lock(&self.log).played.clone()
    }
}

impl SoundPlayer for FakeSoundPlayer {
    fn play(&self, clip: Arc<SoundClip>) -> PortResult<()> {
        let mut log = lock(&self.log);
        if let Some(error) = log.next_error.take() {
            return Err(error);
        }
        log.played.push(clip);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::AppError;

    #[test]
    fn records_clips_unless_told_to_fail() {
        let player = FakeSoundPlayer::default();
        let clip = Arc::new(SoundClip {
            sample_rate: 8_000,
            samples: vec![0.0; 8],
        });
        player.fail_next(AppError::Internal.into());
        assert!(player.play(Arc::clone(&clip)).is_err());
        player.play(Arc::clone(&clip)).unwrap();
        assert_eq!(player.played(), [clip]);
    }
}
