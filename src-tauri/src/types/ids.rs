/*!
 * SOURCE OF TRUTH KEYWORDS: TranscriptId, ULID, EngineId, ModelId, SettingKey, HotkeyId, NavId, MetricId, AudioDeviceId, registry ids, static_str_id
 * WHAT:  Identifier newtypes: TranscriptId (a ULID), the string ids registry entries are keyed by
 *        (EngineId, ModelId, SettingKey, HotkeyId, NavId, MetricId) and the id of an audio input device
 *        (AudioDeviceId).
 *        All serialize as plain strings.
 * WHY:   Distinct types stop an engine id being passed where a model id is expected. ULIDs sort by creation time,
 *        so the transcripts primary key doubles as a history cursor (02 §7.2). Registry ids wrap
 *        StaticStr: `const` registry entries borrow a literal, and ids arriving over IPC deserialize
 *        into an owned string without a second type.
 * WHERE: Every layer. Registry entries build them with `from_static`; commands receive them in inputs.
 */

use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use specta::Type;
use thiserror::Error;
use ulid::Ulid;

/**
 * SOURCE OF TRUTH KEYWORDS: static_str_id macro, string id newtype, StaticStr, const id
 * WHAT:  Declares a transparent StaticStr id newtype with `from_static` (const), `as_str`, `From<String>` and Display.
 * WHY:   Registry ids, setting keys and language codes share this exact shape; one macro keeps them identical.
 * WHERE: Used below for the string ids, by types/engine.rs (Language), types/hotkey.rs (Shortcut) and
 *        types/model.rs (Sha256Hex).
 */
macro_rules! static_str_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(
            Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash,
            serde::Serialize, serde::Deserialize, specta::Type,
        )]
        #[serde(transparent)]
        pub struct $name($crate::types::StaticStr);

        impl $name {
            /// Builds the id from a string literal; usable in `const` registry entries.
            pub const fn from_static(id: &'static str) -> Self {
                Self($crate::types::StaticStr::new(id))
            }

            pub fn as_str(&self) -> &str {
                self.0.as_str()
            }
        }

        impl From<String> for $name {
            fn from(id: String) -> Self {
                Self($crate::types::StaticStr::from(id))
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.0.as_str())
            }
        }
    };
}
pub(crate) use static_str_id;

static_str_id! {
    /// Registry id of an engine entry (ASR, polisher or VAD), kebab-case, e.g. `parakeet-tdt-0.6b-v3`.
    EngineId
}

static_str_id! {
    /// Registry id of a model manifest, kebab-case.
    ModelId
}

static_str_id! {
    /// Registry key of a setting, `section.snake_key`, e.g. `output.auto_paste`.
    SettingKey
}

static_str_id! {
    /// Registry id of a hotkey binding, kebab-case, e.g. `record`, `paste-last`, `cancel`.
    HotkeyId
}

static_str_id! {
    /// Registry id of a sidebar navigation item, kebab-case, e.g. `dashboard`.
    NavId
}

static_str_id! {
    /// Registry id of a dashboard metric, kebab-case, e.g. `time-saved`.
    MetricId
}

static_str_id! {
    /// Stable id of an audio input device as its capture adapter reports it; stored by `audio.input_device`.
    AudioDeviceId
}

/// Primary key of one take: a ULID, so ids sort by creation time.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Type,
)]
#[serde(transparent)]
pub struct TranscriptId(#[specta(type = String)] Ulid);

impl TranscriptId {
    /// A new id stamped with the current time.
    pub fn generate() -> Self {
        Self(Ulid::generate())
    }
}

impl fmt::Display for TranscriptId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// A string that is not a valid id of the expected kind.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("not a valid transcript id")]
pub struct InvalidId;

impl FromStr for TranscriptId {
    type Err = InvalidId;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Ulid::from_string(text).map(Self).map_err(|_| InvalidId)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transcript_id_round_trips_through_its_string_form() {
        let id = TranscriptId::generate();
        let text = id.to_string();
        assert_eq!(text.len(), 26);
        assert_eq!(text.parse::<TranscriptId>().unwrap(), id);
        assert_eq!(serde_json::to_string(&id).unwrap(), format!("\"{text}\""));
        assert_eq!(
            serde_json::from_str::<TranscriptId>(&format!("\"{text}\"")).unwrap(),
            id
        );
    }

    #[test]
    fn transcript_id_rejects_malformed_text() {
        assert_eq!("not-a-ulid".parse::<TranscriptId>(), Err(InvalidId));
        assert!(serde_json::from_str::<TranscriptId>("\"123\"").is_err());
    }

    #[test]
    fn transcript_ids_sort_by_creation_time() {
        let earlier: TranscriptId = "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap();
        let later: TranscriptId = "01BX5ZZKBKACTAV9WEVGEMMVRZ".parse().unwrap();
        assert!(earlier < later);
    }

    #[test]
    fn static_and_owned_registry_ids_are_equal() {
        const STATIC: EngineId = EngineId::from_static("parakeet-tdt-0.6b-v3");
        let owned = EngineId::from(String::from("parakeet-tdt-0.6b-v3"));
        assert_eq!(STATIC, owned);
        assert_eq!(owned.as_str(), "parakeet-tdt-0.6b-v3");
        assert_eq!(
            serde_json::to_string(&STATIC).unwrap(),
            "\"parakeet-tdt-0.6b-v3\""
        );
        assert_eq!(
            serde_json::from_str::<ModelId>("\"m\"").unwrap(),
            ModelId::from_static("m")
        );
        assert_eq!(
            SettingKey::from_static("output.auto_paste").to_string(),
            "output.auto_paste"
        );
    }
}
