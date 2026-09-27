/*!
 * SOURCE OF TRUTH KEYWORDS: event catalog, registry events, AppearanceChanged, AudioDevicesChanged, OnboardingRequested, HotkeyRehearsed, HotkeyStatusChanged, PillLookChanged, tauri_specta Event, collect_events, event names, emit, AppEvent dispatch
 * WHAT:  The catalog of every Rust → UI event: gives each types/events.rs payload its wire name, collects them
 *        for the tauri-specta builder, and emits an AppEvent as its typed payload.
 * WHY:   An event is a registry entry (02 §3.3): adding one is a payload struct plus an AppEvent variant in types/
 *        and one line here.
 *        The `Event` impls live here instead of on the types so types/ carries no framework trait. The wire name
 *        is the struct name (PascalCase past tense, 03 §3); the generated TS exposes it as `events.<camelCase>`.
 * WHERE: `catalog()` is read by app/bindings.rs (export + mount); `emit` is called by app/events.rs
 *        (TauriEventSink), which every command and pipeline emitter reaches through `EventSink<AppEvent>`.
 */

use tauri::{AppHandle, Runtime};
use tauri_specta::{Event, Events, collect_events};

use crate::types::{
    AppEvent, AppearanceChanged, AudioDevicesChanged, AudioLevel, HistoryChanged, HotkeyRehearsed,
    HotkeyStatusChanged, MetricsChanged, ModelProgress, ModelsChanged, NavigationRequested,
    OnboardingRequested, PillLookChanged, SessionStateChanged, SettingsChanged, TranscriptSaved,
};

/**
 * SOURCE OF TRUTH KEYWORDS: event_catalog macro, Event impl, wire event name, emit AppEvent, event dispatch
 * WHAT:  Implements `tauri_specta::Event` for each payload with its struct name as the wire name, and builds
 *        `catalog()`, `NAMES` and `emit` (AppEvent → the payload's typed emit) from the same list.
 * WHY:   One list feeds the impls, the catalog and the dispatcher, so an event can't be named but left
 *        unregistered (tauri-specta panics when an unregistered event is emitted). `emit` matches AppEvent
 *        exhaustively: a payload added to AppEvent but not here, or listed here but missing from AppEvent,
 *        fails to compile.
 * WHERE: Expanded once below.
 */
macro_rules! event_catalog {
    ($($payload:ident),* $(,)?) => {
        $(impl Event for $payload {
            const NAME: &'static str = stringify!($payload);
        })*

        /// Every event, ready for `tauri_specta::Builder::events`.
        pub fn catalog() -> Events {
            collect_events![$($payload),*]
        }

        /// Wire names of every event, in catalog order.
        pub const NAMES: &[&str] = &[$(stringify!($payload)),*];

        /// Sends `event` to every window under its catalog name; the catalog must be mounted first.
        pub fn emit<R: Runtime>(app: &AppHandle<R>, event: AppEvent) -> tauri::Result<()> {
            match event {
                $(AppEvent::$payload(payload) => payload.emit(app),)*
            }
        }
    };
}

event_catalog![
    SessionStateChanged,
    AudioLevel,
    TranscriptSaved,
    HistoryChanged,
    MetricsChanged,
    SettingsChanged,
    ModelProgress,
    ModelsChanged,
    AppearanceChanged,
    NavigationRequested,
    AudioDevicesChanged,
    OnboardingRequested,
    HotkeyRehearsed,
    HotkeyStatusChanged,
    PillLookChanged,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_lists_every_documented_event_once() {
        assert_eq!(
            NAMES,
            [
                "SessionStateChanged",
                "AudioLevel",
                "TranscriptSaved",
                "HistoryChanged",
                "MetricsChanged",
                "SettingsChanged",
                "ModelProgress",
                "ModelsChanged",
                "AppearanceChanged",
                "NavigationRequested",
                "AudioDevicesChanged",
                "OnboardingRequested",
                "HotkeyRehearsed",
                "HotkeyStatusChanged",
                "PillLookChanged",
            ]
        );
        assert_eq!(SessionStateChanged::NAME, "SessionStateChanged");
        // collect_events! panics on a duplicate name, so building the catalog proves uniqueness.
        let _ = catalog();
    }
}
