/*!
 * SOURCE OF TRUTH KEYWORDS: ports layer, port traits, swappable interfaces, AsrEngine, AudioCapture, HotkeyService, SystemAppearance, EventSink, fakes
 * WHAT:  Layer 1: one trait per swappable concern (AI engines, audio, OS integrations, I/O), traits only,
 *        re-exported flat so callers write `use crate::ports::{AsrEngine, Clipboard}`. In test builds, `fakes`
 *        adds one in-memory double per port.
 * WHY:   The core depends on these traits, never on a concrete adapter, so an engine or OS integration can be
 *        replaced without touching pipeline or ipc. No structs and no logic here: caps and every data shape the
 *        traits exchange live in types/ (02 §3.4). Every method returns PortResult (a types/ error), never panics.
 *        Ports shared as `Arc<dyn Port>` are `Send + Sync`; the stateful per-stream ones (VoiceActivity,
 *        CaptureStream, AudioSink) are owned by one worker and only `Send`. Only I/O-waiting ports are async
 *        (BoxFuture). The fakes are `#[cfg(test)]`, so they never ship; they live here because pipeline and ipc
 *        tests may import ports but not adapters (02 §3.2).
 * WHERE: Implemented by adapters/ (and fakes/ in tests); held by pipeline/ and ipc/, built by registry/.
 *        May import types/ only.
 */

mod appearance;
mod asr;
mod audio;
mod clipboard;
mod consent;
mod event_sink;
mod foreground;
mod hotkey;
mod inserter;
mod model_store;
mod notifier;
mod polish;
mod power;
mod updater;
mod vad;

#[cfg(test)]
pub mod fakes;

pub use appearance::SystemAppearance;
pub use asr::AsrEngine;
pub use audio::{AudioCapture, AudioSink, CaptureStream};
pub use clipboard::Clipboard;
pub use consent::PrivacyConsent;
pub use event_sink::EventSink;
pub use foreground::ForegroundApp;
pub use hotkey::HotkeyService;
pub use inserter::TextInserter;
pub use model_store::ModelStore;
pub use notifier::Notifier;
pub use polish::TextPolisher;
pub use power::PowerEvents;
pub use updater::Updater;
pub use vad::VoiceActivity;

/**
 * SOURCE OF TRUTH KEYWORDS: port object safety test, dyn compatible ports, Send Sync ports
 * WHAT:  Compile-time proof that every port is usable as a trait object with the thread bounds the pipeline needs.
 * WHY:   A generic method or `async fn` added to a port would silently make `dyn Port` impossible and only fail
 *        much later in the registry; this fails the gate at the port itself.
 * WHERE: `cargo test`.
 */
#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::types::{CaptureEvent, HotkeyEvent, ModelProgress, PowerEvent, Transparency};

    fn shared<T: ?Sized + Send + Sync>() {}
    fn owned<T: ?Sized + Send>() {}

    #[test]
    fn ports_are_dyn_compatible_with_their_thread_bounds() {
        shared::<dyn AudioCapture>();
        shared::<dyn AsrEngine>();
        shared::<dyn TextPolisher>();
        shared::<dyn HotkeyService>();
        shared::<dyn Clipboard>();
        shared::<dyn TextInserter>();
        shared::<dyn Notifier>();
        shared::<dyn ForegroundApp>();
        shared::<dyn ModelStore>();
        shared::<dyn PowerEvents>();
        shared::<dyn Updater>();
        shared::<dyn PrivacyConsent>();
        shared::<dyn SystemAppearance>();
        shared::<dyn EventSink<Transparency>>();
        shared::<dyn EventSink<HotkeyEvent>>();
        shared::<dyn EventSink<PowerEvent>>();
        shared::<dyn EventSink<CaptureEvent>>();
        shared::<dyn EventSink<ModelProgress>>();
        owned::<dyn VoiceActivity>();
        owned::<dyn CaptureStream>();
        owned::<dyn AudioSink>();
        shared::<Arc<dyn AsrEngine>>();
    }
}
