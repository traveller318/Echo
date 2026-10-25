/*!
 * SOURCE OF TRUTH KEYWORDS: DeviceWatch Linux, microphone hot-plug poll, POLL_INTERVAL, input device snapshot, default input change
 * WHAT:  DeviceWatch (Linux): a thread that reads the input device list every POLL_INTERVAL and emits
 *        `Added`, `Removed` or `DefaultInputChanged` when it differs from the last read. Dropping it stops and joins.
 * WHY:   cpal has no device-change callback on PulseAudio/PipeWire or ALSA, and Echo's relay (pipeline/audio_devices)
 *        already debounces notices and compares whole lists, so a cheap snapshot diff is enough: two seconds is
 *        quick for a plugged-in headset and costs one short IPC round-trip to the sound server. Only ids are kept,
 *        never names, so the comparison is exact.
 * WHERE: Started by CpalCapture::watch_devices (cpal_capture.rs) on Linux; the Windows build uses device_watch.rs.
 */

use std::{
    collections::BTreeSet,
    sync::{Arc, mpsc},
    thread::{self, JoinHandle},
    time::Duration,
};

use cpal::traits::{DeviceTrait, HostTrait};

use crate::{
    ports::EventSink,
    types::{AppError, EndpointChange, PortError, PortResult},
};

/// How often the device list is read.
const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// The input devices and the default one, as ids.
#[derive(Debug, Default, PartialEq, Eq)]
struct Snapshot {
    inputs: BTreeSet<String>,
    default: Option<String>,
}

impl Snapshot {
    /// Reads the list now; a host that cannot be listed reads as empty, so it recovers on the next poll.
    fn read() -> Self {
        let Ok(host) = super::cpal_capture::host() else {
            return Self::default();
        };
        let default = host
            .default_input_device()
            .and_then(|device| device.id().ok())
            .map(|id| id.to_string());
        let inputs = host
            .input_devices()
            .map(|devices| {
                devices
                    .filter_map(|device| device.id().ok().map(|id| id.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        Self { inputs, default }
    }

    /// The notices that turn `self` into `next`.
    fn changes(&self, next: &Self) -> Vec<EndpointChange> {
        let mut changes = Vec::new();
        if next.inputs.difference(&self.inputs).next().is_some() {
            changes.push(EndpointChange::Added);
        }
        if self.inputs.difference(&next.inputs).next().is_some() {
            changes.push(EndpointChange::Removed);
        }
        if self.default != next.default {
            changes.push(EndpointChange::DefaultInputChanged);
        }
        changes
    }
}

/// A running device poll; dropping it stops and joins its thread.
pub struct DeviceWatch {
    stop: Option<mpsc::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl DeviceWatch {
    /// Starts polling, forwarding changes to `sink`.
    pub fn start(sink: Arc<dyn EventSink<EndpointChange>>) -> PortResult<Self> {
        let (stop, stopped) = mpsc::channel::<()>();
        let thread = thread::Builder::new()
            .name("echo-device-watch".to_owned())
            .spawn(move || {
                let mut last = Snapshot::read();
                while let Err(mpsc::RecvTimeoutError::Timeout) = stopped.recv_timeout(POLL_INTERVAL)
                {
                    let next = Snapshot::read();
                    for change in last.changes(&next) {
                        sink.emit(change);
                    }
                    last = next;
                }
            })
            .map_err(|error| {
                PortError::new(AppError::AudioDevice)
                    .with_detail(format!("audio device changes cannot be watched: {error}"))
            })?;
        Ok(Self {
            stop: Some(stop),
            thread: Some(thread),
        })
    }
}

impl Drop for DeviceWatch {
    fn drop(&mut self) {
        // Dropping the sender wakes the thread at once.
        self.stop.take();
        if let Some(thread) = self.thread.take()
            && thread.join().is_err()
        {
            tracing::error!("the audio device watch thread panicked");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(inputs: &[&str], default: Option<&str>) -> Snapshot {
        Snapshot {
            inputs: inputs.iter().map(|id| (*id).to_owned()).collect(),
            default: default.map(str::to_owned),
        }
    }

    #[test]
    fn differences_become_notices() {
        let before = snapshot(&["a", "b"], Some("a"));
        assert!(before.changes(&snapshot(&["a", "b"], Some("a"))).is_empty());
        assert_eq!(
            before.changes(&snapshot(&["a", "b", "c"], Some("a"))),
            [EndpointChange::Added]
        );
        assert_eq!(
            before.changes(&snapshot(&["a"], Some("a"))),
            [EndpointChange::Removed]
        );
        assert_eq!(
            before.changes(&snapshot(&["b", "c"], Some("c"))),
            [
                EndpointChange::Added,
                EndpointChange::Removed,
                EndpointChange::DefaultInputChanged
            ]
        );
    }

    #[test]
    fn the_watch_starts_and_stops() {
        let sink = Arc::new(crate::ports::fakes::RecordingSink::default());
        drop(DeviceWatch::start(sink).unwrap());
    }
}
