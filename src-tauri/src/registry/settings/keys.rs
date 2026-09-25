/*!
 * SOURCE OF TRUTH KEYWORDS: setting key constants, typed setting keys, keys module
 * WHAT:  One constant per setting key.
 * WHY:   Code that reads a setting names it through these, so a typo or a renamed key is a compile error.
 * WHERE: registry/settings (list, reads); registry/permissions; ipc/commands and pipeline tests.
 */

use crate::types::SettingKey;

pub const LAUNCH_AT_STARTUP: SettingKey = SettingKey::from_static("general.launch_at_startup");
pub const START_MINIMIZED: SettingKey = SettingKey::from_static("general.start_minimized");
pub const SOUND_CUES: SettingKey = SettingKey::from_static("general.sound_cues");
pub const THEME: SettingKey = SettingKey::from_static("general.theme");
pub const RECORD_HOTKEY: SettingKey = SettingKey::from_static("hotkeys.record");
pub const HOTKEY_MODE: SettingKey = SettingKey::from_static("hotkeys.mode");
pub const PASTE_LAST_HOTKEY: SettingKey = SettingKey::from_static("hotkeys.paste_last");
pub const CANCEL_COUNTDOWN_MS: SettingKey = SettingKey::from_static("session.cancel_countdown_ms");
pub const MAX_DURATION_MIN: SettingKey = SettingKey::from_static("session.max_duration_min");
pub const INPUT_DEVICE: SettingKey = SettingKey::from_static("audio.input_device");
/// Hidden: the Bluetooth microphone hint (05 W11) was shown.
pub const BLUETOOTH_HINT_SHOWN: SettingKey = SettingKey::from_static("audio.bluetooth_hint_shown");
pub const AUTO_PASTE: SettingKey = SettingKey::from_static("output.auto_paste");
pub const KEEP_ON_CLIPBOARD: SettingKey = SettingKey::from_static("output.keep_on_clipboard");
pub const TRAILING_SPACE: SettingKey = SettingKey::from_static("output.trailing_space");
pub const ASR_ENGINE: SettingKey = SettingKey::from_static("transcription.engine");
pub const LANGUAGE: SettingKey = SettingKey::from_static("transcription.language");
pub const ACCELERATOR: SettingKey = SettingKey::from_static("transcription.accelerator");
pub const REMOVE_FILLERS: SettingKey = SettingKey::from_static("polish.remove_fillers");
pub const DICTIONARY: SettingKey = SettingKey::from_static("polish.dictionary");
pub const LLM_ENABLED: SettingKey = SettingKey::from_static("polish.llm_enabled");
pub const LLM_ENGINE: SettingKey = SettingKey::from_static("polish.llm_engine");
pub const AUDIO_RETENTION_DAYS: SettingKey =
    SettingKey::from_static("storage.audio_retention_days");
pub const HISTORY_RETENTION_DAYS: SettingKey =
    SettingKey::from_static("storage.history_retention_days");
pub const TYPING_WPM: SettingKey = SettingKey::from_static("metrics.typing_wpm");
pub const OFFLINE_MODE: SettingKey = SettingKey::from_static("privacy.offline_mode");
pub const UPDATES_AUTO_CHECK: SettingKey = SettingKey::from_static("updates.auto_check");
