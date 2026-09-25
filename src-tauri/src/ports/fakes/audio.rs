/*!
 * SOURCE OF TRUTH KEYWORDS: FakeAudioCapture, RecordingAudioSink, fake microphone, feed samples, play on start, lose device, fake capture stream, fake hot-plug, fake device transport
 * WHAT:  FakeAudioCapture: an AudioCapture whose "microphone" is driven by the test (`feed`, `play_on_start`,
 *        `lose_device`, `fail_stream`) and whose device list can change under a watch (`set_devices` notifies the
 *        watch sink, like a hot-plug); RecordingAudioSink: an AudioSink that keeps every sample.
 * WHY:   Pipeline tests must produce exact audio at exact moments (speech, silence, an unplug mid-take). Each
 *        `start` bumps a generation, so dropping an old stream handle never closes a newer stream. `play_on_start`
 *        serves code that opens the stream itself (a command handler): the samples reach the sink the moment it opens.
 * WHERE: pipeline capture and session actor tests.
 */

use std::sync::{Arc, Mutex};

use super::lock;
use crate::{
    ports::{AudioCapture, AudioSink, CaptureStream, EventSink},
    types::{
        AppError, AudioCaps, AudioDevice, AudioDeviceId, AudioTransport, CaptureEvent,
        CaptureFormat, EndpointChange, PortError, PortResult, ResourceKind, StaticList,
    },
};

#[derive(Default)]
struct CaptureState {
    sink: Option<Box<dyn AudioSink>>,
    events: Option<Arc<dyn EventSink<CaptureEvent>>>,
    generation: u64,
    paused: bool,
    last_device: Option<Option<AudioDeviceId>>,
    starts: usize,
    next_error: Option<PortError>,
    on_start: Vec<f32>,
    devices: Vec<AudioDevice>,
    /// Transport of the Windows default device (a stream opened without a pinned device).
    default_transport: AudioTransport,
    watch: Option<Arc<dyn EventSink<EndpointChange>>>,
}

impl CaptureState {
    fn close(&mut self, generation: u64) {
        if self.generation == generation {
            self.sink = None;
            self.events = None;
            self.paused = false;
        }
    }

    fn is_open(&self) -> bool {
        self.sink.is_some()
    }
}

/// A scriptable microphone.
pub struct FakeAudioCapture {
    format: CaptureFormat,
    state: Arc<Mutex<CaptureState>>,
}

impl FakeAudioCapture {
    /// A fake whose streams deliver `format`, with no devices listed (only the system default can be opened).
    pub fn new(format: CaptureFormat) -> Self {
        Self {
            format,
            state: Arc::default(),
        }
    }

    #[must_use]
    pub fn with_devices(self, devices: Vec<AudioDevice>) -> Self {
        lock(&self.state).devices = devices;
        self
    }

    /// Replaces the device list, as a plug or unplug would, and tells the watch sink (if any) with `change`.
    pub fn set_devices(&self, devices: Vec<AudioDevice>, change: EndpointChange) {
        let watch = {
            let mut state = lock(&self.state);
            state.devices = devices;
            state.watch.clone()
        };
        if let Some(watch) = watch {
            watch.emit(change);
        }
    }

    /// Streams opened on the Windows default report `transport`.
    pub fn set_default_transport(&self, transport: AudioTransport) {
        lock(&self.state).default_transport = transport;
    }

    /// A device watch is running.
    pub fn is_watched(&self) -> bool {
        lock(&self.state).watch.is_some()
    }

    /// The next `start` fails with `error`.
    pub fn fail_next_start(&self, error: PortError) {
        lock(&self.state).next_error = Some(error);
    }

    /// Every stream opened from now on receives `samples` as soon as it starts.
    pub fn play_on_start(&self, samples: Vec<f32>) {
        lock(&self.state).on_start = samples;
    }

    /// Delivers samples as the microphone would; returns false when no running stream received them.
    pub fn feed(&self, samples: &[f32]) -> bool {
        let mut state = lock(&self.state);
        if state.paused {
            return false;
        }
        match state.sink.as_mut() {
            Some(sink) => {
                sink.push(samples);
                true
            }
            None => false,
        }
    }

    /// Unplugs the device of the open stream.
    pub fn lose_device(&self) {
        self.close_with(CaptureEvent::DeviceLost);
    }

    /// Fails the open stream.
    pub fn fail_stream(&self, error: PortError) {
        self.close_with(CaptureEvent::Failed(error));
    }

    fn close_with(&self, event: CaptureEvent) {
        let mut state = lock(&self.state);
        if let Some(events) = state.events.clone() {
            let generation = state.generation;
            state.close(generation);
            drop(state);
            events.emit(event);
        }
    }

    pub fn is_open(&self) -> bool {
        lock(&self.state).is_open()
    }

    pub fn is_paused(&self) -> bool {
        lock(&self.state).paused
    }

    /// The device requested by the last `start` (inner None = system default), or None before any start.
    pub fn last_device(&self) -> Option<Option<AudioDeviceId>> {
        lock(&self.state).last_device.clone()
    }

    pub fn starts(&self) -> usize {
        lock(&self.state).starts
    }
}

impl AudioCapture for FakeAudioCapture {
    fn caps(&self) -> AudioCaps {
        AudioCaps {
            sample_rates: StaticList::from(vec![self.format.sample_rate]),
            channels: StaticList::from(vec![self.format.channels]),
        }
    }

    fn devices(&self) -> PortResult<Vec<AudioDevice>> {
        Ok(lock(&self.state).devices.clone())
    }

    fn watch_devices(&self, sink: Arc<dyn EventSink<EndpointChange>>) -> PortResult<()> {
        lock(&self.state).watch = Some(sink);
        Ok(())
    }

    fn start(
        &self,
        device: Option<&AudioDeviceId>,
        sink: Box<dyn AudioSink>,
        events: Arc<dyn EventSink<CaptureEvent>>,
    ) -> PortResult<Box<dyn CaptureStream>> {
        let mut state = lock(&self.state);
        state.last_device = Some(device.cloned());
        if let Some(error) = state.next_error.take() {
            return Err(error);
        }
        if state.is_open() {
            return Err(AppError::Busy.into());
        }
        let transport = match device {
            Some(id) => match state.devices.iter().find(|known| &known.id == id) {
                Some(known) => known.transport,
                None => {
                    return Err(AppError::NotFound {
                        resource: ResourceKind::AudioDevice,
                    }
                    .into());
                }
            },
            None => state.default_transport,
        };
        state.generation += 1;
        state.starts += 1;
        let mut sink = sink;
        if !state.on_start.is_empty() {
            sink.push(&state.on_start);
        }
        state.sink = Some(sink);
        state.events = Some(events);
        state.paused = false;
        Ok(Box::new(FakeCaptureStream {
            format: self.format,
            transport,
            generation: state.generation,
            state: Arc::clone(&self.state),
        }))
    }
}

/// The handle `FakeAudioCapture::start` returns.
struct FakeCaptureStream {
    format: CaptureFormat,
    transport: AudioTransport,
    generation: u64,
    state: Arc<Mutex<CaptureState>>,
}

impl FakeCaptureStream {
    fn set_paused(&self, paused: bool) -> PortResult<()> {
        let mut state = lock(&self.state);
        if state.generation != self.generation || !state.is_open() {
            return Err(PortError::new(AppError::AudioDevice).with_detail("fake: stream is closed"));
        }
        state.paused = paused;
        Ok(())
    }
}

impl CaptureStream for FakeCaptureStream {
    fn format(&self) -> CaptureFormat {
        self.format
    }

    fn transport(&self) -> AudioTransport {
        self.transport
    }

    fn pause(&mut self) -> PortResult<()> {
        self.set_paused(true)
    }

    fn resume(&mut self) -> PortResult<()> {
        self.set_paused(false)
    }

    fn stop(self: Box<Self>) -> PortResult<()> {
        lock(&self.state).close(self.generation);
        Ok(())
    }
}

impl Drop for FakeCaptureStream {
    fn drop(&mut self) {
        lock(&self.state).close(self.generation);
    }
}

/// An AudioSink that keeps every sample; clones share the same buffer.
#[derive(Clone, Default)]
pub struct RecordingAudioSink {
    samples: Arc<Mutex<Vec<f32>>>,
}

impl RecordingAudioSink {
    pub fn samples(&self) -> Vec<f32> {
        lock(&self.samples).clone()
    }
}

impl AudioSink for RecordingAudioSink {
    fn push(&mut self, samples: &[f32]) {
        lock(&self.samples).extend_from_slice(samples);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::fakes::RecordingSink;

    const FORMAT: CaptureFormat = CaptureFormat {
        sample_rate: 48_000,
        channels: 2,
    };

    fn open(
        capture: &FakeAudioCapture,
    ) -> (
        Box<dyn CaptureStream>,
        RecordingAudioSink,
        Arc<RecordingSink<CaptureEvent>>,
    ) {
        let sink = RecordingAudioSink::default();
        let events = Arc::new(RecordingSink::default());
        let stream = capture
            .start(None, Box::new(sink.clone()), events.clone())
            .unwrap();
        (stream, sink, events)
    }

    #[test]
    fn samples_flow_only_while_running() {
        let capture = FakeAudioCapture::new(FORMAT);
        assert!(!capture.feed(&[0.1]));
        let (mut stream, sink, _) = open(&capture);
        assert_eq!(stream.format(), FORMAT);
        assert!(capture.feed(&[0.1, 0.2]));
        stream.pause().unwrap();
        assert!(!capture.feed(&[0.3]));
        stream.resume().unwrap();
        assert!(capture.feed(&[0.4]));
        stream.stop().unwrap();
        assert!(!capture.is_open());
        assert_eq!(sink.samples(), [0.1, 0.2, 0.4]);
    }

    #[test]
    fn play_on_start_reaches_every_new_stream() {
        let capture = FakeAudioCapture::new(FORMAT);
        capture.play_on_start(vec![0.25, -0.25]);
        let (stream, sink, _) = open(&capture);
        assert_eq!(sink.samples(), [0.25, -0.25]);
        drop(stream);
        let (_again, second, _) = open(&capture);
        assert_eq!(second.samples(), [0.25, -0.25]);
    }

    #[test]
    fn one_stream_at_a_time_and_drop_closes() {
        let capture = FakeAudioCapture::new(FORMAT);
        let (stream, _, _) = open(&capture);
        let second = capture.start(
            None,
            Box::new(RecordingAudioSink::default()),
            Arc::new(RecordingSink::default()),
        );
        assert_eq!(
            second.err().map(PortError::into_app_error),
            Some(AppError::Busy)
        );
        drop(stream);
        assert!(!capture.is_open());
        let (_again, _, _) = open(&capture);
        assert_eq!(capture.starts(), 2);
    }

    #[test]
    fn a_stale_handle_does_not_close_a_newer_stream() {
        let capture = FakeAudioCapture::new(FORMAT);
        let (mut first, _, _) = open(&capture);
        capture.lose_device();
        let (_second, _, _) = open(&capture);
        assert!(first.pause().is_err());
        drop(first);
        assert!(capture.is_open());
    }

    #[test]
    fn device_loss_is_reported_and_closes_the_stream() {
        let capture = FakeAudioCapture::new(FORMAT);
        let (_stream, _, events) = open(&capture);
        capture.lose_device();
        assert_eq!(events.events(), [CaptureEvent::DeviceLost]);
        assert!(!capture.feed(&[0.5]));
    }

    #[test]
    fn unknown_pinned_devices_are_not_found() {
        let usb = AudioDevice {
            id: AudioDeviceId::from_static("usb"),
            name: String::from("USB"),
            is_default: false,
            transport: AudioTransport::Usb,
        };
        let capture = FakeAudioCapture::new(FORMAT).with_devices(vec![usb.clone()]);
        assert_eq!(capture.devices().unwrap(), std::slice::from_ref(&usb));
        let missing = capture.start(
            Some(&AudioDeviceId::from_static("gone")),
            Box::new(RecordingAudioSink::default()),
            Arc::new(RecordingSink::default()),
        );
        assert_eq!(
            missing.err().map(PortError::into_app_error),
            Some(AppError::NotFound {
                resource: ResourceKind::AudioDevice
            })
        );
        let pinned = capture.start(
            Some(&usb.id),
            Box::new(RecordingAudioSink::default()),
            Arc::new(RecordingSink::default()),
        );
        assert_eq!(pinned.unwrap().transport(), AudioTransport::Usb);
        assert_eq!(capture.last_device(), Some(Some(usb.id)));
    }

    #[test]
    fn a_changed_device_list_reaches_the_watch() {
        let capture = FakeAudioCapture::new(FORMAT);
        capture.set_default_transport(AudioTransport::Bluetooth);
        let (stream, _, _) = open(&capture);
        assert_eq!(stream.transport(), AudioTransport::Bluetooth);
        let watch = Arc::new(RecordingSink::default());
        capture.watch_devices(watch.clone()).unwrap();
        assert!(capture.is_watched());
        let headset = AudioDevice {
            id: AudioDeviceId::from_static("headset"),
            name: String::from("Headset"),
            is_default: true,
            transport: AudioTransport::Bluetooth,
        };
        capture.set_devices(vec![headset.clone()], EndpointChange::Added);
        assert_eq!(watch.events(), [EndpointChange::Added]);
        assert_eq!(capture.devices().unwrap(), [headset]);
    }

    #[test]
    fn scripted_start_failures_surface_once() {
        let capture = FakeAudioCapture::new(FORMAT);
        capture.fail_next_start(
            AppError::PermissionDenied {
                permission: crate::types::Permission::Microphone,
            }
            .into(),
        );
        let blocked = capture.start(
            None,
            Box::new(RecordingAudioSink::default()),
            Arc::new(RecordingSink::default()),
        );
        assert!(blocked.is_err());
        let (_stream, _, _) = open(&capture);
        assert!(capture.is_open());
    }
}
