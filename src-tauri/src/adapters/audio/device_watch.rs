/*!
 * SOURCE OF TRUTH KEYWORDS: DeviceWatch, IMMNotificationClient, RegisterEndpointNotificationCallback, endpoint notifications, hot-plug watch thread, EndpointChange emit
 * WHAT:  DeviceWatch: a thread that registers an IMMNotificationClient with Core Audio and forwards every endpoint
 *        notice (added, removed, state changed, new default capture device) to an EventSink until the watch is
 *        dropped, which unregisters it and joins the thread.
 * WHY:   Windows pushes these notices; nothing needs polling (root CLAUDE.md §7). The COM objects live on one thread
 *        Echo owns, in the multithreaded apartment, so the callbacks (which arrive on Core Audio's own threads) never
 *        need a message pump and no COM pointer crosses threads. A callback only forwards to the sink, which must not
 *        block (ports/event_sink.rs); Core Audio forbids waiting or calling back into the MMDevice API there.
 *        Only a new default *capture* device for the console role is reported: default speakers are irrelevant
 *        and every role fires its own notice. Property changes are ignored: they fire constantly and never add or
 *        remove a microphone. Unregistering waits for a callback in progress, which is safe because callbacks never
 *        wait on this thread. The start reports its own failure (no audio service), so the caller can log it and
 *        carry on with on-demand listing.
 * WHERE: Owned by CpalWasapiCapture (`watch_devices`); started from app/bootstrap with the pipeline's DeviceListRelay.
 */

use std::{
    sync::{Arc, mpsc},
    thread::{self, JoinHandle},
};

use windows::{
    Win32::{
        Foundation::PROPERTYKEY,
        Media::Audio::{
            DEVICE_STATE, EDataFlow, ERole, IMMNotificationClient, IMMNotificationClient_Impl,
            eCapture, eConsole,
        },
    },
    core::{PCWSTR, implement},
};

use super::endpoints;
use crate::{
    adapters::win32::ComScope,
    ports::EventSink,
    types::{AppError, EndpointChange, PortError, PortResult},
};

/// A running endpoint watch; dropping it unregisters and joins its thread.
pub struct DeviceWatch {
    stop: Option<mpsc::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl DeviceWatch {
    /// Registers for endpoint notices, forwarding them to `sink`; returns once registered (or failed).
    pub fn start(sink: Arc<dyn EventSink<EndpointChange>>) -> PortResult<Self> {
        let (stop, stopped) = mpsc::channel::<()>();
        let (ready, registered) = mpsc::channel::<Result<(), String>>();
        let thread = thread::Builder::new()
            .name("echo-device-watch".to_owned())
            .spawn(move || {
                let _com = ComScope::multithreaded();
                let client: IMMNotificationClient = EndpointNotices { sink }.into();
                let enumerator = match endpoints::enumerator().and_then(|enumerator| {
                    // SAFETY: `client` is a live COM object this thread keeps until it unregisters it below.
                    unsafe { enumerator.RegisterEndpointNotificationCallback(&client) }
                        .map(|()| enumerator)
                }) {
                    Ok(enumerator) => enumerator,
                    Err(error) => {
                        let _ = ready.send(Err(error.to_string()));
                        return;
                    }
                };
                let _ = ready.send(Ok(()));
                // Runs until the watch is dropped (the sender goes away).
                let _ = stopped.recv();
                // SAFETY: the same client registered above; waits for a callback in progress, which never waits here.
                if let Err(error) =
                    unsafe { enumerator.UnregisterEndpointNotificationCallback(&client) }
                {
                    tracing::warn!(%error, "the audio device watch could not be unregistered");
                }
            })
            .map_err(|error| watch_failure(&format!("its thread could not start: {error}")))?;
        let mut watch = Self {
            stop: Some(stop),
            thread: Some(thread),
        };
        match registered.recv() {
            Ok(Ok(())) => Ok(watch),
            Ok(Err(detail)) => {
                watch.stop();
                Err(watch_failure(&detail))
            }
            Err(_) => {
                watch.stop();
                Err(watch_failure("its thread ended before registering"))
            }
        }
    }

    fn stop(&mut self) {
        self.stop = None;
        if let Some(thread) = self.thread.take()
            && thread.join().is_err()
        {
            tracing::error!("the audio device watch thread panicked");
        }
    }
}

impl Drop for DeviceWatch {
    fn drop(&mut self) {
        self.stop();
    }
}

fn watch_failure(detail: &str) -> PortError {
    PortError::new(AppError::AudioDevice)
        .with_detail(format!("audio device changes cannot be watched: {detail}"))
}

/// The COM object Core Audio calls back.
#[implement(IMMNotificationClient)]
struct EndpointNotices {
    sink: Arc<dyn EventSink<EndpointChange>>,
}

impl IMMNotificationClient_Impl for EndpointNotices_Impl {
    fn OnDeviceStateChanged(&self, _: &PCWSTR, _: DEVICE_STATE) -> windows::core::Result<()> {
        self.sink.emit(EndpointChange::StateChanged);
        Ok(())
    }

    fn OnDeviceAdded(&self, _: &PCWSTR) -> windows::core::Result<()> {
        self.sink.emit(EndpointChange::Added);
        Ok(())
    }

    fn OnDeviceRemoved(&self, _: &PCWSTR) -> windows::core::Result<()> {
        self.sink.emit(EndpointChange::Removed);
        Ok(())
    }

    fn OnDefaultDeviceChanged(
        &self,
        flow: EDataFlow,
        role: ERole,
        _: &PCWSTR,
    ) -> windows::core::Result<()> {
        if flow == eCapture && role == eConsole {
            self.sink.emit(EndpointChange::DefaultInputChanged);
        }
        Ok(())
    }

    fn OnPropertyValueChanged(&self, _: &PCWSTR, _: &PROPERTYKEY) -> windows::core::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::fakes::RecordingSink;

    #[test]
    fn a_watch_registers_and_unregisters_cleanly() {
        let sink = Arc::new(RecordingSink::<EndpointChange>::default());
        let watch = DeviceWatch::start(Arc::clone(&sink) as _).unwrap();
        drop(watch);
        // A second watch after the first one is gone registers again.
        drop(DeviceWatch::start(sink as _).unwrap());
    }

    #[test]
    fn callbacks_forward_only_what_the_pipeline_needs() {
        let sink = Arc::new(RecordingSink::<EndpointChange>::default());
        let client: IMMNotificationClient = EndpointNotices {
            sink: Arc::clone(&sink) as _,
        }
        .into();
        let id = PCWSTR::null();
        // SAFETY: calling our own COM object through its interface, with a null id it never reads.
        unsafe {
            client.OnDeviceAdded(id).unwrap();
            client.OnDeviceRemoved(id).unwrap();
            client.OnDeviceStateChanged(id, DEVICE_STATE(1)).unwrap();
            client
                .OnDefaultDeviceChanged(windows::Win32::Media::Audio::eRender, eConsole, id)
                .unwrap();
            client
                .OnDefaultDeviceChanged(eCapture, windows::Win32::Media::Audio::eCommunications, id)
                .unwrap();
            client
                .OnDefaultDeviceChanged(eCapture, eConsole, id)
                .unwrap();
            client
                .OnPropertyValueChanged(id, PROPERTYKEY::default())
                .unwrap();
        }
        assert_eq!(
            sink.events(),
            [
                EndpointChange::Added,
                EndpointChange::Removed,
                EndpointChange::StateChanged,
                EndpointChange::DefaultInputChanged,
            ]
        );
    }
}
