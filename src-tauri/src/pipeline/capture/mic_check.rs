/*!
 * SOURCE OF TRUTH KEYWORDS: check_microphone, mic check, audio_test_level, microphone test window, level verdict, device lost during check
 * WHAT:  `check_microphone` listens to a device for a window through the regular capture worker (levels only: no
 *        journal, no VAD, no AudioLevel events) and returns its peak and mean RMS with a MicVerdict.
 * WHY:   Onboarding and Settings must prove the microphone works before a take depends on it (05 §4 checklist):
 *        reusing Capture means the check hears exactly what a take would (same device selection, downmix,
 *        resampling and errors), including a blocked privacy consent that delivers digital silence (05 W13). The
 *        wait is an async timer, so no runtime thread sleeps; closing the stream and joining the worker afterwards
 *        takes a few milliseconds. A device lost or failing during the window is an error, not a verdict, so the
 *        UI never calls a broken device "quiet".
 * WHERE: `ipc/commands/audio.rs` (`audio_test_level`).
 */

use std::{sync::Arc, time::Duration};

use parking_lot::Mutex;

use super::{Capture, CaptureConfig, level};
use crate::{
    ports::{AudioCapture, EventSink, WorkerScheduler},
    types::{AppError, AudioDeviceId, CaptureEvent, MicCheck, PortError, PortResult},
};

/// Listens to `device` (None = Windows default) for `window` and reports how it sounded.
pub async fn check_microphone(
    capture: &dyn AudioCapture,
    scheduler: Arc<dyn WorkerScheduler>,
    device: Option<&AudioDeviceId>,
    window: Duration,
) -> PortResult<MicCheck> {
    let stream_events = Arc::new(FirstEvent::default());
    let take = Capture::start(
        capture,
        device,
        CaptureConfig {
            journal: None,
            segmentation: None,
            levels: None,
            events: stream_events.clone(),
            scheduler,
        },
    )?;
    tokio::time::sleep(window).await;
    let outcome = take.finish();
    if let Some(failure) = outcome.failure {
        return Err(failure);
    }
    match stream_events.take() {
        Some(CaptureEvent::DeviceLost) => Err(PortError::new(AppError::AudioDevice)
            .with_detail("the device disappeared during the microphone check")),
        Some(CaptureEvent::Failed(error)) => Err(error),
        None => Ok(MicCheck {
            peak_rms: outcome.summary.peak_rms,
            mean_rms: outcome.summary.mean_rms,
            verdict: level::verdict(outcome.summary.peak_rms),
        }),
    }
}

/// Keeps the first event a stream reports; later ones add nothing to the check's answer.
#[derive(Default)]
struct FirstEvent(Mutex<Option<CaptureEvent>>);

impl FirstEvent {
    fn take(&self) -> Option<CaptureEvent> {
        self.0.lock().take()
    }
}

impl EventSink<CaptureEvent> for FirstEvent {
    fn emit(&self, event: CaptureEvent) {
        self.0.lock().get_or_insert(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ports::fakes::{FakeAudioCapture, FakeWorkerScheduler},
        types::{CaptureFormat, MicVerdict, ResourceKind},
    };

    const FORMAT: CaptureFormat = CaptureFormat {
        sample_rate: 48_000,
        channels: 2,
    };

    fn run(capture: &FakeAudioCapture, device: Option<&AudioDeviceId>) -> PortResult<MicCheck> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        runtime.block_on(check_microphone(
            capture,
            Arc::new(FakeWorkerScheduler::default()),
            device,
            Duration::from_millis(30),
        ))
    }

    #[test]
    fn a_speaking_microphone_is_good() {
        let capture = FakeAudioCapture::new(FORMAT);
        capture.play_on_start(vec![0.2; 48_000]);
        let check = run(&capture, None).unwrap();
        assert!((check.peak_rms - 0.2).abs() < 0.01);
        assert_eq!(check.verdict, MicVerdict::Good);
        assert!(
            !capture.is_open(),
            "the microphone is closed after the check"
        );
    }

    #[test]
    fn a_silent_or_missing_stream_is_no_signal() {
        let capture = FakeAudioCapture::new(FORMAT);
        let check = run(&capture, None).unwrap();
        assert_eq!(check.verdict, MicVerdict::NoSignal);
        assert_eq!(check.peak_rms, 0.0);
    }

    #[test]
    fn an_unknown_device_is_not_found() {
        let capture = FakeAudioCapture::new(FORMAT);
        let error = run(&capture, Some(&AudioDeviceId::from_static("gone")))
            .err()
            .map(PortError::into_app_error);
        assert_eq!(
            error,
            Some(AppError::NotFound {
                resource: ResourceKind::AudioDevice
            })
        );
    }

    #[test]
    fn the_first_stream_event_is_kept() {
        let events = FirstEvent::default();
        events.emit(CaptureEvent::DeviceLost);
        events.emit(CaptureEvent::Failed(AppError::Internal.into()));
        assert_eq!(events.take(), Some(CaptureEvent::DeviceLost));
        assert_eq!(events.take(), None);
    }
}
