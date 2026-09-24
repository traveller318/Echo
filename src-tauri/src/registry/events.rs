/*!
 * SOURCE OF TRUTH KEYWORDS: event catalog, registry events, tauri_specta Event, collect_events, event names, emit
 * WHAT:  The catalog of every Rust → UI event: gives each types/events.rs payload its wire name and collects them
 *        for the tauri-specta builder.
 * WHY:   An event is a registry entry (02 §3.3): adding one is a payload struct in types/ plus one line here.
 *        The `Event` impls live here instead of on the types so types/ carries no framework trait. The wire name
 *        is the struct name (PascalCase past tense, 03 §3); the generated TS exposes it as `events.<camelCase>`.
 * WHERE: `catalog()` is read by app/bindings.rs (export + mount); emitters call `Payload.emit(&app)` through the
 *        `tauri_specta::Event` trait.
 */

use tauri_specta::{Event, Events, collect_events};

use crate::types::{
    AudioLevel, HistoryChanged, MetricsChanged, ModelProgress, SessionStateChanged,
    SettingsChanged, TranscriptSaved,
};

/**
 * SOURCE OF TRUTH KEYWORDS: event_names macro, Event impl, wire event name
 * WHAT:  Implements `tauri_specta::Event` for each payload with its struct name as the wire name, and builds
 *        `catalog()` from the same list.
 * WHY:   One list feeds both the impls and the catalog, so an event can't be named but left unregistered
 *        (tauri-specta panics when an unregistered event is emitted).
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
            ]
        );
        assert_eq!(SessionStateChanged::NAME, "SessionStateChanged");
        // collect_events! panics on a duplicate name, so building the catalog proves uniqueness.
        let _ = catalog();
    }
}
