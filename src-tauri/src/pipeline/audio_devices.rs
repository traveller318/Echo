/*!
 * SOURCE OF TRUTH KEYWORDS: DeviceListRelay, microphone hot-plug, device list changed, AudioDevicesChanged emit, endpoint notice debounce, DEVICE_SETTLE
 * WHAT:  DeviceListRelay: the EventSink the capture adapter's device watch pushes raw EndpointChange notices into.
 *        A thread of its own waits until the notices settle, lists the microphones again and emits
 *        AudioDevicesChanged only when the list (ids, names, default, transport) differs from the last one it saw.
 * WHY:   Windows reports one plug as a burst of notices, many about speakers, and `emit` runs on an OS callback
 *        thread that must not block or call back into the audio API, so the notice is only queued there. Waiting
 *        DEVICE_SETTLE after the last notice turns a burst into one list read, and comparing lists turns speaker-only
 *        noise into nothing, so the Settings picker refetches once per real change and never polls (root CLAUDE.md
 *        §7). A take needs nothing from here: each take resolves the pinned device or the default when it opens
 *        (pipeline/session/arm.rs), which is what "follow the system default unless pinned" means (02 §9). No list
 *        is read at startup (05 W19: nothing audio before it is needed), so the first settled burst is always
 *        announced; that costs at most one extra refetch and can never miss a change made while the relay started.
 *        A list that cannot be read is logged and the last good list kept. The thread ends when the relay is
 *        dropped.
 * WHERE: Built by app/bootstrap and handed to `AudioCapture::watch_devices`.
 */

use std::{
    sync::{
        Arc,
        mpsc::{self, RecvTimeoutError, Sender},
    },
    thread,
    time::Duration,
};

use crate::{
    ports::{AudioCapture, EventSink},
    types::{
        AppError, AppEvent, AudioDevice, AudioDevicesChanged, EndpointChange, PortError, PortResult,
    },
};

/// Quiet time after the last notice before the list is read again.
pub const DEVICE_SETTLE: Duration = Duration::from_millis(300);

/// Queues endpoint notices for the relay thread.
pub struct DeviceListRelay {
    notices: Sender<EndpointChange>,
}

impl DeviceListRelay {
    /// Starts the relay thread over `audio`, emitting to `events`, with `settle` as the quiet time.
    pub fn spawn(
        audio: Arc<dyn AudioCapture>,
        events: Arc<dyn EventSink<AppEvent>>,
        settle: Duration,
    ) -> PortResult<Self> {
        let (notices, received) = mpsc::channel();
        thread::Builder::new()
            .name("echo-audio-devices".to_owned())
            .spawn(move || {
                let mut known: Option<Vec<AudioDevice>> = None;
                while let Ok(first) = received.recv() {
                    tracing::debug!(change = ?first, "audio endpoints changed");
                    loop {
                        match received.recv_timeout(settle) {
                            Ok(more) => tracing::trace!(change = ?more, "audio endpoints changed"),
                            Err(RecvTimeoutError::Timeout) => break,
                            Err(RecvTimeoutError::Disconnected) => return,
                        }
                    }
                    match sorted(audio.devices()) {
                        Err(error) => tracing::warn!(
                            detail = error.detail(),
                            "the microphones could not be listed after a change"
                        ),
                        Ok(after) => {
                            if known.as_ref() == Some(&after) {
                                continue;
                            }
                            tracing::info!(count = after.len(), "the microphones changed");
                            known = Some(after);
                            events.emit(AudioDevicesChanged {}.into());
                        }
                    }
                }
            })
            .map_err(|error| {
                PortError::new(AppError::Internal)
                    .with_detail(format!("the device relay could not start: {error}"))
            })?;
        Ok(Self { notices })
    }
}

impl EventSink<EndpointChange> for DeviceListRelay {
    fn emit(&self, change: EndpointChange) {
        // The thread only stops once this sender is gone, so the send cannot fail while the relay exists.
        let _ = self.notices.send(change);
    }
}

/// The list in id order, so a reordering by Windows is not a change.
fn sorted(devices: PortResult<Vec<AudioDevice>>) -> PortResult<Vec<AudioDevice>> {
    devices.map(|mut devices| {
        devices.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));
        devices
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ports::fakes::{ChannelSink, FakeAudioCapture},
        types::{AudioDeviceId, AudioTransport, CaptureFormat},
    };

    const QUICK: Duration = Duration::from_millis(40);

    fn device(id: &'static str, is_default: bool) -> AudioDevice {
        AudioDevice {
            id: AudioDeviceId::from_static(id),
            name: id.to_owned(),
            is_default,
            transport: AudioTransport::Usb,
        }
    }

    fn rig(devices: Vec<AudioDevice>) -> (Arc<FakeAudioCapture>, Arc<ChannelSink<AppEvent>>) {
        let audio = Arc::new(
            FakeAudioCapture::new(CaptureFormat {
                sample_rate: 48_000,
                channels: 1,
            })
            .with_devices(devices),
        );
        let events = Arc::new(ChannelSink::default());
        let relay =
            DeviceListRelay::spawn(Arc::clone(&audio) as _, Arc::clone(&events) as _, QUICK)
                .unwrap();
        audio.watch_devices(Arc::new(relay)).unwrap();
        (audio, events)
    }

    #[test]
    fn a_plugged_microphone_is_announced_once_per_burst() {
        let (audio, events) = rig(vec![device("laptop", true)]);
        audio.set_devices(vec![device("laptop", true)], EndpointChange::StateChanged);
        assert_eq!(
            events.next(),
            Some(AppEvent::AudioDevicesChanged(AudioDevicesChanged {})),
            "the baseline"
        );
        let both = vec![device("laptop", false), device("usb", true)];
        audio.set_devices(both.clone(), EndpointChange::Added);
        audio.set_devices(both.clone(), EndpointChange::StateChanged);
        audio.set_devices(both, EndpointChange::DefaultInputChanged);
        assert_eq!(
            events.next(),
            Some(AppEvent::AudioDevicesChanged(AudioDevicesChanged {}))
        );
        assert!(events.nothing_within(QUICK * 5), "one burst, one event");

        audio.set_devices(vec![device("laptop", true)], EndpointChange::Removed);
        assert_eq!(
            events.next(),
            Some(AppEvent::AudioDevicesChanged(AudioDevicesChanged {}))
        );
    }

    #[test]
    fn a_notice_that_changes_no_microphone_is_not_announced() {
        let pair = vec![device("a", true), device("b", false)];
        let (audio, events) = rig(pair.clone());
        // The first burst sets the baseline and is always announced (no list is read at startup).
        audio.set_devices(pair, EndpointChange::StateChanged);
        assert_eq!(
            events.next(),
            Some(AppEvent::AudioDevicesChanged(AudioDevicesChanged {}))
        );
        // Speakers changing, or Windows listing the same microphones in another order.
        audio.set_devices(
            vec![device("b", false), device("a", true)],
            EndpointChange::DefaultInputChanged,
        );
        assert!(events.nothing_within(QUICK * 5));
    }
}
