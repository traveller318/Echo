/*!
 * SOURCE OF TRUTH KEYWORDS: AppError, AppErrorCode, error code, IPC error, HotkeyIssue, ResourceKind, user-safe message
 * WHAT:  AppError, the only error that crosses IPC, plus AppErrorCode, the stable fieldless code of each variant.
 * WHY:   One error surface for the UI (02 §4.2): the frontend maps `code` to copy and an action in one place
 *        (src/lib/app-error.ts). `#[serde(tag = "code")]` makes the variant name the wire code, so a variant is
 *        never renamed: codes are also persisted in `transcripts.error_code`. Fields carry only what the UI needs;
 *        internal detail is logged by the command factory and never serialized. Display text is short, calm and
 *        free of transcript text, because it is what logs and native toasts show.
 * WHERE: Returned by every command through ipc/factory.rs; stored via TranscriptSummary/Transcript.error_code;
 *        carried by SessionView.error.
 */

use serde::{Deserialize, Serialize};
use specta::Type;
use thiserror::Error;

use super::{ModelId, Permission};

/// What a `NotFound` error could not find.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    Transcript,
    Model,
    Engine,
    Setting,
    AudioDevice,
    /// An update to install (the updater found none, or updates are not configured, 02 §11).
    Update,
}

/// Why a hotkey could not be registered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum HotkeyIssue {
    /// The combination is already taken: by another Echo hotkey, or (with a RegisterHotKey backend) by another app
    /// (05 W7).
    Conflict,
    /// The combination cannot be registered (no key, or not allowed by the hotkey adapter's caps).
    Invalid,
}

/// The only error type that crosses IPC. The variant name is the stable wire `code`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, Error)]
#[serde(tag = "code")]
pub enum AppError {
    #[error("{field} is not valid: {message}")]
    Validation { field: String, message: String },
    #[error("permission {permission:?} is not granted")]
    PermissionDenied { permission: Permission },
    #[error("the same operation is already running")]
    Busy,
    #[error("{resource:?} not found")]
    NotFound { resource: ResourceKind },
    #[error("model {model_id} is not installed")]
    ModelMissing { model_id: ModelId },
    #[error("model {model_id} failed verification")]
    ModelCorrupt { model_id: ModelId },
    #[error("the microphone could not be used")]
    AudioDevice,
    #[error("speech recognition failed")]
    Asr,
    #[error("text cleanup failed")]
    Polish,
    #[error("reading or writing local storage failed")]
    Storage,
    #[error("offline mode is on")]
    Offline,
    #[error("the download server could not be reached")]
    Network,
    #[error("hotkey could not be registered: {reason:?}")]
    Hotkey { reason: HotkeyIssue },
    #[error("something went wrong")]
    Internal,
}

/// The stable code of an AppError variant, without its fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
pub enum AppErrorCode {
    Validation,
    PermissionDenied,
    Busy,
    NotFound,
    ModelMissing,
    ModelCorrupt,
    AudioDevice,
    Asr,
    Polish,
    Storage,
    Offline,
    Network,
    Hotkey,
    Internal,
}

impl AppError {
    pub fn validation(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Validation {
            field: field.into(),
            message: message.into(),
        }
    }

    /**
     * SOURCE OF TRUTH KEYWORDS: AppError::code, error code mapping, exhaustive code match
     * WHAT:  The stable code of this error.
     * WHY:   Exhaustive on purpose: a new AppError variant does not compile until it has a code, and the tests
     *        below prove the code equals the serialized `code` tag.
     * WHERE: Services persisting `transcripts.error_code`; the factory's outcome metric.
     */
    pub const fn code(&self) -> AppErrorCode {
        match self {
            Self::Validation { .. } => AppErrorCode::Validation,
            Self::PermissionDenied { .. } => AppErrorCode::PermissionDenied,
            Self::Busy => AppErrorCode::Busy,
            Self::NotFound { .. } => AppErrorCode::NotFound,
            Self::ModelMissing { .. } => AppErrorCode::ModelMissing,
            Self::ModelCorrupt { .. } => AppErrorCode::ModelCorrupt,
            Self::AudioDevice => AppErrorCode::AudioDevice,
            Self::Asr => AppErrorCode::Asr,
            Self::Polish => AppErrorCode::Polish,
            Self::Storage => AppErrorCode::Storage,
            Self::Offline => AppErrorCode::Offline,
            Self::Network => AppErrorCode::Network,
            Self::Hotkey { .. } => AppErrorCode::Hotkey,
            Self::Internal => AppErrorCode::Internal,
        }
    }
}

impl AppErrorCode {
    /// Every code, in declaration order.
    pub const ALL: [Self; 14] = [
        Self::Validation,
        Self::PermissionDenied,
        Self::Busy,
        Self::NotFound,
        Self::ModelMissing,
        Self::ModelCorrupt,
        Self::AudioDevice,
        Self::Asr,
        Self::Polish,
        Self::Storage,
        Self::Offline,
        Self::Network,
        Self::Hotkey,
        Self::Internal,
    ];

    /// The wire and database form of the code.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Validation => "Validation",
            Self::PermissionDenied => "PermissionDenied",
            Self::Busy => "Busy",
            Self::NotFound => "NotFound",
            Self::ModelMissing => "ModelMissing",
            Self::ModelCorrupt => "ModelCorrupt",
            Self::AudioDevice => "AudioDevice",
            Self::Asr => "Asr",
            Self::Polish => "Polish",
            Self::Storage => "Storage",
            Self::Offline => "Offline",
            Self::Network => "Network",
            Self::Hotkey => "Hotkey",
            Self::Internal => "Internal",
        }
    }

    /// Parses a code read back from the database.
    pub fn parse(code: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == code)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use serde_json::{Value, json};

    use super::*;

    /// One sample per variant. `code()` is exhaustive, so the ALL-length check below fails if one is missing.
    fn samples() -> Vec<AppError> {
        vec![
            AppError::validation("hotkeys.record", "Use at least one modifier key."),
            AppError::PermissionDenied {
                permission: Permission::Microphone,
            },
            AppError::Busy,
            AppError::NotFound {
                resource: ResourceKind::Transcript,
            },
            AppError::ModelMissing {
                model_id: ModelId::from_static("parakeet-tdt-0.6b-v3"),
            },
            AppError::ModelCorrupt {
                model_id: ModelId::from_static("parakeet-tdt-0.6b-v3"),
            },
            AppError::AudioDevice,
            AppError::Asr,
            AppError::Polish,
            AppError::Storage,
            AppError::Offline,
            AppError::Network,
            AppError::Hotkey {
                reason: HotkeyIssue::Conflict,
            },
            AppError::Internal,
        ]
    }

    #[test]
    fn every_variant_has_a_distinct_code() {
        let codes: HashSet<AppErrorCode> = samples().iter().map(AppError::code).collect();
        assert_eq!(codes.len(), AppErrorCode::ALL.len());
    }

    #[test]
    fn serialized_code_tag_equals_the_stable_code() {
        for error in samples() {
            let value = serde_json::to_value(&error).unwrap();
            assert_eq!(
                value["code"],
                Value::from(error.code().as_str()),
                "{error:?}"
            );
            assert_eq!(
                serde_json::to_value(error.code()).unwrap(),
                Value::from(error.code().as_str())
            );
        }
    }

    #[test]
    fn codes_are_frozen() {
        let wire: Vec<&str> = AppErrorCode::ALL.iter().map(|code| code.as_str()).collect();
        assert_eq!(
            wire,
            [
                "Validation",
                "PermissionDenied",
                "Busy",
                "NotFound",
                "ModelMissing",
                "ModelCorrupt",
                "AudioDevice",
                "Asr",
                "Polish",
                "Storage",
                "Offline",
                "Network",
                "Hotkey",
                "Internal",
            ]
        );
        for code in AppErrorCode::ALL {
            assert_eq!(AppErrorCode::parse(code.as_str()), Some(code));
        }
        assert_eq!(AppErrorCode::parse("Unknown"), None);
    }

    #[test]
    fn wire_shape_is_flat_and_round_trips() {
        let error = AppError::Hotkey {
            reason: HotkeyIssue::Conflict,
        };
        assert_eq!(
            serde_json::to_value(&error).unwrap(),
            json!({ "code": "Hotkey", "reason": "conflict" })
        );
        assert_eq!(
            serde_json::to_value(AppError::Busy).unwrap(),
            json!({ "code": "Busy" })
        );
        for error in samples() {
            let text = serde_json::to_string(&error).unwrap();
            assert_eq!(serde_json::from_str::<AppError>(&text).unwrap(), error);
        }
    }

    #[test]
    fn messages_are_calm() {
        for error in samples() {
            let message = error.to_string();
            assert!(!message.is_empty());
            assert!(!message.contains('!'), "{message}");
        }
    }
}
