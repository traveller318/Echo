/*!
 * SOURCE OF TRUTH KEYWORDS: TranscriptId, ULID, EngineId, ModelId, SettingKey, HotkeyId, MetricId, AudioDeviceId, registry ids, static_str_id
 * WHAT:  Identifier newtypes: TranscriptId (a ULID), the string ids registry entries are keyed by
 *        (EngineId, ModelId, SettingKey, HotkeyId, MetricId) and the id of an audio input device
 *        (AudioDeviceId).
 *        All serialize as plain strings.
 * WHY:   Distinct types stop an engine id being passed where a model id is expected. ULIDs sort by creation time,
 *        so the transcripts primary key doubles as a history cursor (02 §7.2). Registry ids wrap
 *        StaticStr: `const` registry entries borrow a literal, and ids arriving over IPC deserialize
 *        into an owned string without a second type. Sidebar ids are a closed enum instead (NavId, types/nav.rs),
 *        because the UI must own a page for each one.
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

impl SettingKey {
    /// Longest setting key accepted over IPC, in bytes.
    pub const MAX_LEN: usize = 64;

    /**
     * SOURCE OF TRUTH KEYWORDS: setting key format, section.snake_key, is_well_formed, settings input schema
     * WHAT:  Whether the key has the `section.snake_key` shape (03 §3): two non-empty parts of lowercase ASCII
     *        letters, digits and underscores, each starting with a letter, at most `MAX_LEN` bytes.
     * WHY:   The declared input schema of the settings commands (garde) rejects malformed keys before any lookup,
     *        and the registry test proves every registered key has the same shape, so the two cannot disagree.
     *        Whether the key exists is the registry's answer, not this check's.
     * WHERE: types/settings.rs (`SettingsSetInput`, `SettingsResetInput` garde rules); registry/settings tests.
     */
    pub fn is_well_formed(&self) -> bool {
        let key = self.as_str();
        let part = |text: &str| {
            text.starts_with(|character: char| character.is_ascii_lowercase())
                && text.chars().all(|character| {
                    character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
                })
        };
        key.len() <= Self::MAX_LEN
            && key
                .split_once('.')
                .is_some_and(|(section, name)| part(section) && part(name))
    }
}

static_str_id! {
    /// Registry id of a hotkey binding, kebab-case, e.g. `record`, `paste-last`, `cancel`.
    HotkeyId
}

static_str_id! {
    /// Registry id of a dashboard metric, kebab-case, e.g. `time-saved`.
    MetricId
}

static_str_id! {
    /// Stable id of an audio input device as its capture adapter reports it; stored by `audio.input_device`.
    AudioDeviceId
}

impl AudioDeviceId {
    /// Longest device id accepted over IPC, in bytes (a WASAPI endpoint id is about 60).
    pub const MAX_LEN: usize = 512;

    /**
     * SOURCE OF TRUTH KEYWORDS: device id format, AudioDeviceId is_well_formed, audio input schema
     * WHAT:  Whether the id could have come from a capture adapter: non-empty, at most `MAX_LEN` bytes, no control
     *        characters.
     * WHY:   The declared schema of every audio command input (garde) rejects junk before the adapter parses it;
     *        whether the device is present is the adapter's answer (`NotFound { audio_device }`), not this check's.
     * WHERE: types/audio.rs (`AudioTestLevelInput` garde rule).
     */
    pub fn is_well_formed(&self) -> bool {
        let id = self.as_str();
        !id.is_empty() && id.len() <= Self::MAX_LEN && !id.chars().any(char::is_control)
    }
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
    fn audio_device_ids_must_be_short_printable_text() {
        let valid = AudioDeviceId::from_static("wasapi:{0.0.1.00000000}.{8e5c3a2b}");
        assert!(valid.is_well_formed());
        for invalid in [
            String::new(),
            "a".repeat(AudioDeviceId::MAX_LEN + 1),
            String::from("usb\nmic"),
        ] {
            assert!(!AudioDeviceId::from(invalid).is_well_formed());
        }
    }

    #[test]
    fn setting_keys_must_be_section_dot_snake_key() {
        for valid in [
            "general.theme",
            "output.auto_paste",
            "session.max_duration_min",
            "a1.b_2",
        ] {
            assert!(SettingKey::from_static(valid).is_well_formed(), "{valid}");
        }
        let too_long = format!("general.{}", "a".repeat(SettingKey::MAX_LEN));
        for invalid in [
            "",
            "general",
            "general.",
            ".theme",
            "General.theme",
            "general.theme.extra",
            "general.auto-paste",
            "general._theme",
            "1general.theme",
            "general.thème",
            too_long.as_str(),
        ] {
            assert!(
                !SettingKey::from(invalid.to_owned()).is_well_formed(),
                "{invalid}"
            );
        }
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
