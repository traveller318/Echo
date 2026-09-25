/*!
 * SOURCE OF TRUTH KEYWORDS: CpalWasapiCapture, cpal, WASAPI shared mode, input devices, default input device, device id, capture callback, DeviceLost, sample format conversion, device transport, watch_devices
 * WHAT:  CpalWasapiCapture: AudioCapture on cpal's WASAPI host (shared mode). Lists input devices with a stable id
 *        (cpal's `DeviceId`, persisted as text by `audio.input_device`) and how each is connected, watches for
 *        hot-plug (DeviceWatch), and opens one at its default shared-mode format; the stream handle pauses, resumes
 *        and closes it and says how the opened device is connected.
 * WHY:   Shared mode opens at the device's mix format, so no other app loses the microphone and no exclusive-mode
 *        negotiation delays the first take (05 W14); downmix and resampling to 16 kHz happen once in the pipeline
 *        (05 A2). The data callback runs on cpal's real-time thread and only converts into a scratch buffer
 *        allocated when the stream is built, then pushes to the sink: no allocation, lock or wait (02 §6.1). A
 *        device format other than f32 is converted there in slices of whole frames. Device loss arrives on the
 *        error callback: an unplugged device (`DeviceNotAvailable`) and a replaced default device
 *        (`StreamInvalidated`, cpal never rebinds a WASAPI stream) both end this stream as `DeviceLost`, so the take
 *        keeps what was captured and the next take uses the new default (05 W12). An xrun (a glitch the OS already
 *        recovered from) is only logged. Only the first terminal event is reported. One stream at a time: the port
 *        contract makes a second `start` fail with `Busy`. The Windows privacy consent is read before opening
 *        (05 W13), because a blocked microphone still opens and delivers silence. The transport comes from cpal's
 *        connection type, except that any Windows Bluetooth bus (read from the endpoint, endpoints.rs) is Bluetooth,
 *        because cpal misses the hands-free bus a headset microphone uses (05 W11). COM is entered in cpal's
 *        apartment for that read (ComScope::shared), so cpal's own COM use on the thread is never disturbed.
 * WHERE: Built by app/bootstrap into CommandCtx (and the session actor later); used through `dyn AudioCapture` by
 *        pipeline/capture and `audio_list_devices`.
 */

use std::{
    str::FromStr,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use cpal::{
    Device, DeviceId, ErrorKind, FromSample, Host, HostId, InterfaceType, SampleFormat,
    SizedSample, Stream, StreamConfig,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use parking_lot::Mutex;
use windows::Win32::Media::Audio::IMMDeviceEnumerator;

use super::{device_watch::DeviceWatch, endpoints};
use crate::{
    adapters::win32::ComScope,
    ports::{AudioCapture, AudioSink, CaptureStream, EventSink, PrivacyConsent},
    types::{
        AppError, AudioCaps, AudioDevice, AudioDeviceId, AudioTransport, CaptureEvent,
        CaptureFormat, EndpointChange, Permission, PermissionState, PortError, PortResult,
        ResourceKind, StaticList,
    },
};

/// Sample rates a Windows shared-mode mix format uses.
const SAMPLE_RATES: &[u32] = &[
    8_000, 11_025, 16_000, 22_050, 32_000, 44_100, 48_000, 88_200, 96_000, 176_400, 192_000,
];

/// Channel counts of Windows capture endpoints (mono, stereo, arrays and surround layouts).
const CHANNELS: &[u16] = &[1, 2, 4, 6, 8];

/// Frames converted per sink push when the device format is not f32.
const CONVERT_FRAMES: usize = 1024;

/// Microphone capture through WASAPI shared mode.
pub struct CpalWasapiCapture {
    consent: Arc<dyn PrivacyConsent>,
    /// A stream is open; cleared when its handle is dropped.
    open: Arc<AtomicBool>,
    /// The hot-plug watch, once started.
    watch: Mutex<Option<DeviceWatch>>,
}

impl CpalWasapiCapture {
    /// `consent` is read before every open (05 W13).
    pub fn new(consent: Arc<dyn PrivacyConsent>) -> Self {
        Self {
            consent,
            open: Arc::new(AtomicBool::new(false)),
            watch: Mutex::new(None),
        }
    }
}

impl AudioCapture for CpalWasapiCapture {
    fn caps(&self) -> AudioCaps {
        AudioCaps {
            sample_rates: StaticList::new(SAMPLE_RATES),
            channels: StaticList::new(CHANNELS),
        }
    }

    fn devices(&self) -> PortResult<Vec<AudioDevice>> {
        let buses = Buses::open();
        let host = host()?;
        let default_id = host
            .default_input_device()
            .and_then(|device| device.id().ok());
        let devices = host
            .input_devices()
            .map_err(|error| device_failure("list input devices", &error))?;
        Ok(devices
            .filter_map(|device| {
                // A device that vanishes while it is listed simply drops out of the list.
                let id = device.id().ok()?;
                let (name, transport) = describe(&device, &buses);
                Some(AudioDevice {
                    is_default: default_id.as_ref() == Some(&id),
                    name,
                    id: AudioDeviceId::from(id.to_string()),
                    transport,
                })
            })
            .collect())
    }

    fn watch_devices(&self, sink: Arc<dyn EventSink<EndpointChange>>) -> PortResult<()> {
        let mut watch = self.watch.lock();
        // The previous watch unregisters (and joins) before the new one registers.
        *watch = None;
        *watch = Some(DeviceWatch::start(sink)?);
        Ok(())
    }

    fn start(
        &self,
        device: Option<&AudioDeviceId>,
        sink: Box<dyn AudioSink>,
        events: Arc<dyn EventSink<CaptureEvent>>,
    ) -> PortResult<Box<dyn CaptureStream>> {
        if self.consent.microphone()? == PermissionState::Denied {
            return Err(microphone_blocked());
        }
        if self.open.swap(true, Ordering::AcqRel) {
            return Err(
                PortError::new(AppError::Busy).with_detail("a capture stream is already open")
            );
        }
        let opened = open_stream(device, sink, events);
        if opened.is_err() {
            self.open.store(false, Ordering::Release);
        }
        let (stream, format, transport) = opened?;
        Ok(Box::new(CpalCaptureStream {
            stream,
            format,
            transport,
            open: Arc::clone(&self.open),
        }))
    }
}

/// Opens `device` (None = the Windows default input) at its default shared-mode format and starts it; returns the
/// stream, its format and how the opened device is connected.
fn open_stream(
    device: Option<&AudioDeviceId>,
    sink: Box<dyn AudioSink>,
    events: Arc<dyn EventSink<CaptureEvent>>,
) -> PortResult<(Stream, CaptureFormat, AudioTransport)> {
    let host = host()?;
    let device = match device {
        Some(id) => find_device(&host, id)?,
        None => host.default_input_device().ok_or_else(|| {
            PortError::new(AppError::AudioDevice).with_detail("Windows has no default input device")
        })?,
    };
    let supported = device
        .default_input_config()
        .map_err(|error| open_failure(&error))?;
    let config: StreamConfig = supported.config();
    let format = CaptureFormat {
        sample_rate: config.sample_rate,
        channels: config.channels,
    };
    let stream = match supported.sample_format() {
        SampleFormat::F32 => build::<f32>(&device, &config, sink, events),
        SampleFormat::I16 => build::<i16>(&device, &config, sink, events),
        SampleFormat::I32 => build::<i32>(&device, &config, sink, events),
        SampleFormat::U8 => build::<u8>(&device, &config, sink, events),
        SampleFormat::U16 => build::<u16>(&device, &config, sink, events),
        SampleFormat::F64 => build::<f64>(&device, &config, sink, events),
        other => {
            return Err(PortError::new(AppError::AudioDevice)
                .with_detail(format!("unsupported device sample format {other:?}")));
        }
    }
    .map_err(|error| open_failure(&error))?;
    stream.play().map_err(|error| open_failure(&error))?;
    let (_, transport) = describe(&device, &Buses::open());
    Ok((stream, format, transport))
}

/**
 * SOURCE OF TRUTH KEYWORDS: build input stream, real-time callback, scratch conversion buffer, error callback, first terminal event
 * WHAT:  Builds a cpal input stream of sample type T whose callback hands f32 to `sink` and whose error callback
 *        reports device loss or failure once to `events`.
 * WHY:   f32 slices go straight to the sink; other formats are converted into a scratch buffer sized here (whole
 *        frames), because the callback may not allocate. The error callback may fire repeatedly after a device
 *        dies, so a flag keeps the report to one event.
 * WHERE: open_stream above.
 */
fn build<T>(
    device: &Device,
    config: &StreamConfig,
    mut sink: Box<dyn AudioSink>,
    events: Arc<dyn EventSink<CaptureEvent>>,
) -> Result<Stream, cpal::Error>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let channels = usize::from(config.channels.max(1));
    let mut scratch = vec![0.0_f32; CONVERT_FRAMES * channels];
    let ended = AtomicBool::new(false);
    let on_error = move |error: cpal::Error| match error.kind() {
        ErrorKind::Xrun => tracing::debug!("audio input glitch (xrun)"),
        kind => {
            if ended.swap(true, Ordering::AcqRel) {
                return;
            }
            events.emit(match kind {
                ErrorKind::DeviceNotAvailable
                | ErrorKind::StreamInvalidated
                | ErrorKind::DeviceChanged => CaptureEvent::DeviceLost,
                _ => CaptureEvent::Failed(device_failure("keep capturing", &error)),
            });
        }
    };
    device.build_input_stream::<T, _, _>(
        *config,
        move |data: &[T], _| {
            for chunk in data.chunks(scratch.len()) {
                let converted = &mut scratch[..chunk.len()];
                for (out, sample) in converted.iter_mut().zip(chunk) {
                    *out = sample.to_sample::<f32>();
                }
                sink.push(converted);
            }
        },
        on_error,
        None,
    )
}

/// The open WASAPI stream. Dropping it stops the device thread after its current callback.
struct CpalCaptureStream {
    stream: Stream,
    format: CaptureFormat,
    transport: AudioTransport,
    open: Arc<AtomicBool>,
}

impl CaptureStream for CpalCaptureStream {
    fn format(&self) -> CaptureFormat {
        self.format
    }

    fn transport(&self) -> AudioTransport {
        self.transport
    }

    fn pause(&mut self) -> PortResult<()> {
        self.stream
            .pause()
            .map_err(|error| device_failure("pause the microphone", &error))
    }

    fn resume(&mut self) -> PortResult<()> {
        self.stream
            .play()
            .map_err(|error| device_failure("resume the microphone", &error))
    }

    fn stop(self: Box<Self>) -> PortResult<()> {
        // Dropping joins cpal's device thread, so every sample it captured has reached the sink on return.
        drop(self);
        Ok(())
    }
}

impl Drop for CpalCaptureStream {
    fn drop(&mut self) {
        self.open.store(false, Ordering::Release);
    }
}

fn host() -> PortResult<Host> {
    cpal::host_from_id(HostId::Wasapi).map_err(|error| device_failure("open WASAPI", &error))
}

/// The input device with `id`; `NotFound { audio_device }` when it is unplugged or the id is not one of cpal's.
fn find_device(host: &Host, id: &AudioDeviceId) -> PortResult<Device> {
    let missing = || {
        PortError::new(AppError::NotFound {
            resource: ResourceKind::AudioDevice,
        })
        .with_detail(format!("input device `{id}` is not present"))
    };
    let parsed = DeviceId::from_str(id.as_str()).map_err(|_| missing())?;
    host.device_by_id(&parsed).ok_or_else(missing)
}

/**
 * SOURCE OF TRUTH KEYWORDS: Buses, describe device, device name, device transport, interface type mapping
 * WHAT:  Buses: COM entered in cpal's apartment plus an MMDevice enumerator, for reading endpoint buses during one
 *        listing or open. `describe(device, buses)`: the name Windows shows (e.g. `Microphone (USB Audio)`) and how
 *        the device is connected.
 * WHY:   One cpal description per device gives both the name and cpal's connection type; a Windows Bluetooth bus
 *        overrides it (see the file header). An enumerator that cannot be created only means no Bluetooth override.
 * WHERE: `devices` and `open_stream`.
 */
struct Buses {
    enumerator: Option<IMMDeviceEnumerator>,
    // Dropped after the enumerator (field order), so COM outlives every interface.
    _com: ComScope,
}

impl Buses {
    fn open() -> Self {
        let com = ComScope::shared();
        Self {
            enumerator: endpoints::enumerator().ok(),
            _com: com,
        }
    }

    fn is_bluetooth(&self, device: &Device) -> bool {
        let Some(enumerator) = &self.enumerator else {
            return false;
        };
        device
            .id()
            .ok()
            .and_then(|id| endpoints::bus_of(enumerator, id.id()))
            .is_some_and(|bus| endpoints::is_bluetooth_bus(&bus))
    }
}

fn describe(device: &Device, buses: &Buses) -> (String, AudioTransport) {
    let description = device.description().ok();
    let name = description.as_ref().map_or_else(
        || device.to_string(),
        |description| description.name().to_owned(),
    );
    let transport = if buses.is_bluetooth(device) {
        AudioTransport::Bluetooth
    } else {
        description.map_or(AudioTransport::Other, |description| {
            transport_of(description.interface_type())
        })
    };
    (name, transport)
}

/// cpal's connection type as an AudioTransport.
fn transport_of(interface: InterfaceType) -> AudioTransport {
    match interface {
        InterfaceType::BuiltIn | InterfaceType::Pci => AudioTransport::BuiltIn,
        InterfaceType::Usb => AudioTransport::Usb,
        InterfaceType::Bluetooth => AudioTransport::Bluetooth,
        InterfaceType::Virtual | InterfaceType::Aggregate => AudioTransport::Virtual,
        _ => AudioTransport::Other,
    }
}

/// Maps a failure to open the device to the port's error meanings.
fn open_failure(error: &cpal::Error) -> PortError {
    match error.kind() {
        ErrorKind::PermissionDenied => microphone_blocked().with_detail(error.to_string()),
        ErrorKind::DeviceNotAvailable => PortError::new(AppError::NotFound {
            resource: ResourceKind::AudioDevice,
        })
        .with_detail(format!("the input device went away while opening: {error}")),
        _ => device_failure("open the microphone", error),
    }
}

fn microphone_blocked() -> PortError {
    PortError::new(AppError::PermissionDenied {
        permission: Permission::Microphone,
    })
    .with_detail("Windows privacy settings block microphone access for desktop apps")
}

fn device_failure(action: &str, error: &dyn std::fmt::Display) -> PortError {
    PortError::new(AppError::AudioDevice).with_detail(format!("could not {action}: {error}"))
}

/**
 * SOURCE OF TRUTH KEYWORDS: cpal adapter tests, blocked microphone test, device listing test
 * WHAT:  Contract tests that hold on any machine: a blocked privacy consent fails before any device is touched,
 *        device ids round-trip through cpal's parser, and listing never fails when WASAPI is present.
 * WHY:   Build machines may have no microphone, so nothing here requires one; opening a real device is covered
 *        by the manual E2E checklist (05 §4).
 * WHERE: `cargo test`.
 */
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::fakes::{FakePrivacyConsent, RecordingAudioSink, RecordingSink};

    fn capture(consent: FakePrivacyConsent) -> CpalWasapiCapture {
        CpalWasapiCapture::new(Arc::new(consent))
    }

    #[test]
    fn a_blocked_microphone_fails_before_opening_a_device() {
        let capture = capture(FakePrivacyConsent::new(PermissionState::Denied));
        let result = capture.start(
            None,
            Box::new(RecordingAudioSink::default()),
            Arc::new(RecordingSink::default()),
        );
        assert_eq!(
            result.err().map(PortError::into_app_error),
            Some(AppError::PermissionDenied {
                permission: Permission::Microphone
            })
        );
        assert!(!capture.open.load(Ordering::Acquire));
    }

    #[test]
    fn listed_devices_have_ids_cpal_can_find_again() {
        let capture = capture(FakePrivacyConsent::granted());
        let devices = capture.devices().unwrap();
        assert!(devices.iter().filter(|device| device.is_default).count() <= 1);
        let host = host().unwrap();
        for device in &devices {
            assert!(device.id.is_well_formed());
            assert!(find_device(&host, &device.id).is_ok(), "{}", device.id);
        }
    }

    #[test]
    fn an_unknown_pinned_device_is_not_found() {
        let host = host().unwrap();
        for id in [
            "wasapi:{0.0.1.00000000}.{00000000-0000-0000-0000-000000000000}",
            "no-host",
        ] {
            assert_eq!(
                find_device(&host, &AudioDeviceId::from(id.to_owned()))
                    .err()
                    .map(PortError::into_app_error),
                Some(AppError::NotFound {
                    resource: ResourceKind::AudioDevice
                })
            );
        }
    }

    #[test]
    fn connection_types_map_to_transports() {
        assert_eq!(
            transport_of(InterfaceType::BuiltIn),
            AudioTransport::BuiltIn
        );
        assert_eq!(transport_of(InterfaceType::Pci), AudioTransport::BuiltIn);
        assert_eq!(transport_of(InterfaceType::Usb), AudioTransport::Usb);
        assert_eq!(
            transport_of(InterfaceType::Bluetooth),
            AudioTransport::Bluetooth
        );
        assert_eq!(
            transport_of(InterfaceType::Virtual),
            AudioTransport::Virtual
        );
        assert_eq!(transport_of(InterfaceType::Hdmi), AudioTransport::Other);
    }

    #[test]
    fn the_device_watch_starts_and_can_be_replaced() {
        let capture = capture(FakePrivacyConsent::granted());
        capture
            .watch_devices(Arc::new(RecordingSink::default()))
            .unwrap();
        capture
            .watch_devices(Arc::new(RecordingSink::default()))
            .unwrap();
        assert!(capture.watch.lock().is_some());
    }

    #[test]
    fn caps_cover_the_common_shared_mode_formats() {
        let caps = capture(FakePrivacyConsent::granted()).caps();
        assert!(caps.sample_rates.contains(&48_000) && caps.sample_rates.contains(&44_100));
        assert!(caps.channels.contains(&1) && caps.channels.contains(&2));
    }
}
