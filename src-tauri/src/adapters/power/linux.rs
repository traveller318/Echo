/*!
 * SOURCE OF TRUTH KEYWORDS: LogindPowerEvents, logind, PrepareForSleep, session Unlock, org.freedesktop.login1, D-Bus power events, sleep and wake Linux
 * WHAT:  LogindPowerEvents: PowerEvents over systemd-logind on the system bus: `PrepareForSleep(true)` is Suspend,
 *        `PrepareForSleep(false)` is Resume, and the `Unlock` signal of the caller's own session is SessionResumed.
 *        `listen` starts a thread with its own small async runtime that forwards the signals until the adapter is
 *        dropped; a second `listen` only replaces the sink.
 * WHY:   logind is what suspends the machine and locks the session on every systemd desktop (and elogind on the
 *        others speaks the same interface), so it is the one place both are announced. A take open when the machine
 *        sleeps must be finalized before it does (02 §9). The session path `/org/freedesktop/login1/session/auto`
 *        names the caller's session without looking it up. The connection runs on its own thread so a slow bus
 *        never blocks Tauri's runtime; a missing logind (a container, a non-systemd system without elogind) fails
 *        `listen`, which bootstrap logs, and Echo works without sleep handling.
 * WHERE: Built and kept alive by app/bootstrap.rs `watch_power` on Linux; the session actor consumes the events.
 */

use std::{
    sync::{Arc, mpsc},
    thread::{self, JoinHandle},
};

use futures_util::StreamExt;
use parking_lot::Mutex;
use tokio::sync::oneshot;

use crate::{
    ports::{EventSink, PowerEvents},
    types::{AppError, PortError, PortResult, PowerEvent},
};

type SharedSink = Arc<Mutex<Option<Arc<dyn EventSink<PowerEvent>>>>>;

/// Sleep, wake and unlock notices from logind.
#[derive(Default)]
pub struct LogindPowerEvents {
    sink: SharedSink,
    worker: Mutex<Option<Worker>>,
}

struct Worker {
    stop: Option<oneshot::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl LogindPowerEvents {
    pub fn new() -> Self {
        Self::default()
    }
}

impl PowerEvents for LogindPowerEvents {
    fn listen(&self, sink: Arc<dyn EventSink<PowerEvent>>) -> PortResult<()> {
        *self.sink.lock() = Some(sink);
        let mut worker = self.worker.lock();
        if worker.is_some() {
            return Ok(());
        }
        let (stop, stopped) = oneshot::channel();
        let (ready, subscribed) = mpsc::channel::<Result<(), String>>();
        let sink = Arc::clone(&self.sink);
        let thread = thread::Builder::new()
            .name("echo-power-watch".to_owned())
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        drop(ready.send(Err(error.to_string())));
                        return;
                    }
                };
                runtime.block_on(watch(sink, ready, stopped));
            })
            .map_err(|error| failure(&error.to_string()))?;
        match subscribed.recv() {
            Ok(Ok(())) => {
                *worker = Some(Worker {
                    stop: Some(stop),
                    thread: Some(thread),
                });
                Ok(())
            }
            Ok(Err(detail)) => {
                drop(thread.join());
                Err(failure(&detail))
            }
            Err(_) => {
                drop(thread.join());
                Err(failure("the power watch thread ended before subscribing"))
            }
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(thread) = self.thread.take()
            && thread.join().is_err()
        {
            tracing::error!("the power watch thread panicked");
        }
    }
}

/// Subscribes to logind's signals, reports the outcome on `ready`, then forwards them until `stopped`.
async fn watch(
    sink: SharedSink,
    ready: mpsc::Sender<Result<(), String>>,
    stopped: oneshot::Receiver<()>,
) {
    let subscribed = async {
        let bus = zbus::Connection::system().await?;
        let manager = zbus::Proxy::new(
            &bus,
            "org.freedesktop.login1",
            "/org/freedesktop/login1",
            "org.freedesktop.login1.Manager",
        )
        .await?;
        let session = zbus::Proxy::new(
            &bus,
            "org.freedesktop.login1",
            "/org/freedesktop/login1/session/auto",
            "org.freedesktop.login1.Session",
        )
        .await?;
        let sleep = manager.receive_signal("PrepareForSleep").await?;
        let unlock = session.receive_signal("Unlock").await?;
        zbus::Result::Ok((bus, sleep, unlock))
    }
    .await;
    let (_bus, mut sleep, mut unlock) = match subscribed {
        Ok(streams) => {
            drop(ready.send(Ok(())));
            streams
        }
        Err(error) => {
            drop(ready.send(Err(error.to_string())));
            return;
        }
    };
    let emit = |event| {
        if let Some(sink) = sink.lock().as_ref() {
            sink.emit(event);
        }
    };
    tokio::pin!(stopped);
    loop {
        tokio::select! {
            _ = &mut stopped => return,
            Some(message) = sleep.next() => match message.body().deserialize::<bool>() {
                Ok(starting) => emit(if starting { PowerEvent::Suspend } else { PowerEvent::Resume }),
                Err(error) => tracing::debug!(%error, "unreadable PrepareForSleep signal"),
            },
            Some(_) = unlock.next() => emit(PowerEvent::SessionResumed),
            else => return,
        }
    }
}

fn failure(detail: &str) -> PortError {
    PortError::new(AppError::Internal)
        .with_detail(format!("logind power events unavailable: {detail}"))
}
