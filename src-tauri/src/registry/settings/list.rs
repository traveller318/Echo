/*!
 * SOURCE OF TRUTH KEYWORDS: SETTINGS list, setting specs, setting defaults, theme options, hotkey mode options, dictionary limits
 * WHAT:  SETTINGS: every setting spec in Settings page order, plus the fixed option lists and limits they use.
 * WHY:   Adding a setting is one entry here; the Settings UI, its Zod schema and write validation are generated
 *        from it (root CLAUDE.md §7).
 * WHERE: Re-exported by registry/settings; read by resolve.rs, options.rs, `registry_get` and the settings tests.
 */

use super::{keys, values};
use crate::{
    registry::hotkeys,
    types::{
        CapsRequirement, EnumOption, EnumOptions, OptionSource, SettingKind, SettingSection,
        SettingSpec, SettingUnit, SettingValue, StaticList, StaticStr, ThemePreference,
    },
};

pub(super) const THEME_OPTIONS: &[EnumOption] = &[
    EnumOption {
        value: StaticStr::new(ThemePreference::System.as_str()),
        label: StaticStr::new("Match Windows"),
        requires: None,
    },
    EnumOption {
        value: StaticStr::new(ThemePreference::Light.as_str()),
        label: StaticStr::new("Light"),
        requires: None,
    },
    EnumOption {
        value: StaticStr::new(ThemePreference::Dark.as_str()),
        label: StaticStr::new("Dark"),
        requires: None,
    },
];

const HOTKEY_MODE_OPTIONS: &[EnumOption] = &[
    EnumOption {
        value: StaticStr::new(values::TOGGLE),
        label: StaticStr::new("Toggle"),
        requires: None,
    },
    EnumOption {
        value: StaticStr::new(values::HOLD),
        label: StaticStr::new("Hold to talk"),
        requires: Some(CapsRequirement::HotkeyRelease),
    },
];

/// `polish.dictionary` limits (02 §3.3: at most 500 entries).
const DICTIONARY_MAX_PAIRS: u32 = 500;
const DICTIONARY_MAX_LEN: u32 = 100;

/// Every setting, in Settings page order (sections appear in the order of their first setting).
pub const SETTINGS: &[SettingSpec] = &[
    SettingSpec {
        key: keys::LAUNCH_AT_STARTUP,
        section: SettingSection::General,
        label: StaticStr::new("Launch at startup"),
        help: StaticStr::new("Start Echo when you sign in to Windows."),
        kind: SettingKind::Bool,
        default: SettingValue::Bool(true),
        restart_required: false,
        visible: true,
        requires: None,
    },
    SettingSpec {
        key: keys::START_MINIMIZED,
        section: SettingSection::General,
        label: StaticStr::new("Start in the tray"),
        help: StaticStr::new(
            "Keep this window hidden when Echo launches. Open it from the tray icon.",
        ),
        kind: SettingKind::Bool,
        default: SettingValue::Bool(true),
        restart_required: false,
        visible: true,
        requires: None,
    },
    SettingSpec {
        key: keys::SOUND_CUES,
        section: SettingSection::General,
        label: StaticStr::new("Sound cues"),
        help: StaticStr::new("Play a short sound when recording starts, stops or is cancelled."),
        kind: SettingKind::Bool,
        default: SettingValue::Bool(true),
        restart_required: false,
        visible: true,
        requires: None,
    },
    SettingSpec {
        key: keys::THEME,
        section: SettingSection::General,
        label: StaticStr::new("Appearance"),
        help: StaticStr::new("Follow Windows, or always use light or dark."),
        kind: SettingKind::Enum {
            options: EnumOptions::Fixed {
                list: StaticList::new(THEME_OPTIONS),
            },
        },
        default: SettingValue::Enum(StaticStr::new(ThemePreference::System.as_str())),
        restart_required: false,
        visible: true,
        requires: None,
    },
    SettingSpec {
        key: keys::RECORD_HOTKEY,
        section: SettingSection::Hotkeys,
        label: StaticStr::new("Dictation hotkey"),
        help: StaticStr::new("Starts and stops dictation from any app."),
        kind: SettingKind::Hotkey,
        default: SettingValue::Hotkey(StaticStr::new(hotkeys::RECORD_DEFAULT)),
        restart_required: false,
        visible: true,
        requires: None,
    },
    SettingSpec {
        key: keys::HOTKEY_MODE,
        section: SettingSection::Hotkeys,
        label: StaticStr::new("Hotkey mode"),
        help: StaticStr::new(
            "Toggle: press once to start and again to stop. Hold to talk: record only while the keys are held.",
        ),
        kind: SettingKind::Enum {
            options: EnumOptions::Fixed {
                list: StaticList::new(HOTKEY_MODE_OPTIONS),
            },
        },
        default: SettingValue::Enum(StaticStr::new(values::HOLD)),
        restart_required: false,
        visible: true,
        requires: None,
    },
    SettingSpec {
        key: keys::PASTE_LAST_HOTKEY,
        section: SettingSection::Hotkeys,
        label: StaticStr::new("Paste last hotkey"),
        help: StaticStr::new("Pastes your most recent transcript again."),
        kind: SettingKind::Hotkey,
        default: SettingValue::Hotkey(StaticStr::new(hotkeys::PASTE_LAST_DEFAULT)),
        restart_required: false,
        visible: true,
        requires: None,
    },
    SettingSpec {
        key: keys::CANCEL_COUNTDOWN_MS,
        section: SettingSection::Session,
        label: StaticStr::new("Cancel countdown"),
        help: StaticStr::new(
            "How long Esc waits before discarding a take. Press Esc again during the countdown to keep recording.",
        ),
        kind: SettingKind::Int {
            min: 1000,
            max: 10_000,
            unit: Some(SettingUnit::Milliseconds),
        },
        default: SettingValue::Int(3000),
        restart_required: false,
        visible: true,
        requires: None,
    },
    SettingSpec {
        key: keys::MAX_DURATION_MIN,
        section: SettingSection::Session,
        label: StaticStr::new("Longest take"),
        help: StaticStr::new(
            "Recording stops by itself after this long, and the text is delivered.",
        ),
        kind: SettingKind::Int {
            min: 1,
            max: 60,
            unit: Some(SettingUnit::Minutes),
        },
        default: SettingValue::Int(15),
        restart_required: false,
        visible: true,
        requires: None,
    },
    SettingSpec {
        key: keys::INPUT_DEVICE,
        section: SettingSection::Audio,
        label: StaticStr::new("Microphone"),
        help: StaticStr::new("The input Echo records from. System default follows Windows."),
        kind: SettingKind::Device,
        default: SettingValue::Device(None),
        restart_required: false,
        visible: true,
        requires: None,
    },
    SettingSpec {
        key: keys::AUTO_PASTE,
        section: SettingSection::Output,
        label: StaticStr::new("Paste automatically"),
        help: StaticStr::new("Paste the text into the app you were using as soon as you stop."),
        kind: SettingKind::Bool,
        default: SettingValue::Bool(true),
        restart_required: false,
        visible: true,
        requires: None,
    },
    SettingSpec {
        key: keys::KEEP_ON_CLIPBOARD,
        section: SettingSection::Output,
        label: StaticStr::new("Keep on clipboard"),
        help: StaticStr::new(
            "Leave the text on the clipboard after pasting. It is never added to Windows clipboard history.",
        ),
        kind: SettingKind::Bool,
        default: SettingValue::Bool(true),
        restart_required: false,
        visible: true,
        requires: None,
    },
    SettingSpec {
        key: keys::TRAILING_SPACE,
        section: SettingSection::Output,
        label: StaticStr::new("Add a trailing space"),
        help: StaticStr::new("End every take with a space so the next one follows naturally."),
        kind: SettingKind::Bool,
        default: SettingValue::Bool(true),
        restart_required: false,
        visible: true,
        requires: None,
    },
    SettingSpec {
        key: keys::ASR_ENGINE,
        section: SettingSection::Transcription,
        label: StaticStr::new("Speech engine"),
        help: StaticStr::new("The local model that turns your speech into text."),
        kind: SettingKind::Enum {
            options: EnumOptions::Runtime {
                source: OptionSource::AsrEngines,
            },
        },
        default: SettingValue::Enum(StaticStr::new("parakeet-tdt-0.6b-v3")),
        restart_required: false,
        visible: true,
        requires: None,
    },
    SettingSpec {
        key: keys::LANGUAGE,
        section: SettingSection::Transcription,
        label: StaticStr::new("Language"),
        help: StaticStr::new("Auto-detect, or pick the language you speak for better accuracy."),
        kind: SettingKind::Enum {
            options: EnumOptions::Runtime {
                source: OptionSource::AsrLanguages,
            },
        },
        default: SettingValue::Enum(StaticStr::new(values::AUTO)),
        restart_required: false,
        visible: true,
        requires: Some(CapsRequirement::MultipleLanguages),
    },
    SettingSpec {
        key: keys::ACCELERATOR,
        section: SettingSection::Transcription,
        label: StaticStr::new("Processor"),
        help: StaticStr::new(
            "Automatic tries the graphics card and keeps whichever is faster on this PC.",
        ),
        kind: SettingKind::Enum {
            options: EnumOptions::Runtime {
                source: OptionSource::AsrAccelerators,
            },
        },
        default: SettingValue::Enum(StaticStr::new(values::AUTO)),
        restart_required: false,
        visible: true,
        requires: Some(CapsRequirement::GpuAccelerator),
    },
    SettingSpec {
        key: keys::REMOVE_FILLERS,
        section: SettingSection::Polish,
        label: StaticStr::new("Remove filler words"),
        help: StaticStr::new("Drop fillers such as \u{201c}um\u{201d} and \u{201c}uh\u{201d}."),
        kind: SettingKind::Bool,
        default: SettingValue::Bool(true),
        restart_required: false,
        visible: true,
        requires: None,
    },
    SettingSpec {
        key: keys::DICTIONARY,
        section: SettingSection::Polish,
        label: StaticStr::new("Dictionary"),
        help: StaticStr::new(
            "Words Echo should always write your way, such as names and terms. Matching ignores case.",
        ),
        kind: SettingKind::Pairs {
            max_pairs: DICTIONARY_MAX_PAIRS,
            max_len: DICTIONARY_MAX_LEN,
        },
        default: SettingValue::Pairs(StaticList::new(&[])),
        restart_required: false,
        visible: true,
        requires: None,
    },
    SettingSpec {
        key: keys::LLM_ENABLED,
        section: SettingSection::Polish,
        label: StaticStr::new("Grammar polish"),
        help: StaticStr::new(
            "Use a local language model to fix grammar. Needs a one-time download and adds a short delay.",
        ),
        kind: SettingKind::Bool,
        default: SettingValue::Bool(false),
        restart_required: false,
        visible: true,
        requires: None,
    },
    SettingSpec {
        key: keys::LLM_ENGINE,
        section: SettingSection::Polish,
        label: StaticStr::new("Polish model"),
        help: StaticStr::new("The local language model used for grammar polish."),
        kind: SettingKind::Enum {
            options: EnumOptions::Runtime {
                source: OptionSource::ModelPolishers,
            },
        },
        default: SettingValue::Enum(StaticStr::new("qwen3-1.7b")),
        restart_required: false,
        visible: true,
        requires: None,
    },
    SettingSpec {
        key: keys::AUDIO_RETENTION_DAYS,
        section: SettingSection::Storage,
        label: StaticStr::new("Keep recordings"),
        help: StaticStr::new(
            "Days to keep audio after a successful take. 0 deletes it right away. Failed takes keep their audio.",
        ),
        kind: SettingKind::Int {
            min: 0,
            max: 365,
            unit: Some(SettingUnit::Days),
        },
        default: SettingValue::Int(7),
        restart_required: false,
        visible: true,
        requires: None,
    },
    SettingSpec {
        key: keys::HISTORY_RETENTION_DAYS,
        section: SettingSection::Storage,
        label: StaticStr::new("Keep history"),
        help: StaticStr::new(
            "Days to keep transcripts. Dashboard totals count only what is kept. 0 keeps them forever.",
        ),
        kind: SettingKind::Int {
            min: 0,
            max: 3650,
            unit: Some(SettingUnit::Days),
        },
        default: SettingValue::Int(30),
        restart_required: false,
        visible: true,
        requires: None,
    },
    SettingSpec {
        key: keys::TYPING_WPM,
        section: SettingSection::Metrics,
        label: StaticStr::new("Typing speed"),
        help: StaticStr::new("Your usual typing speed, used to work out the time Echo saves you."),
        kind: SettingKind::Int {
            min: 10,
            max: 200,
            unit: Some(SettingUnit::WordsPerMinute),
        },
        default: SettingValue::Int(40),
        restart_required: false,
        visible: true,
        requires: None,
    },
    SettingSpec {
        key: keys::OFFLINE_MODE,
        section: SettingSection::Privacy,
        label: StaticStr::new("Offline mode"),
        help: StaticStr::new(
            "Block every network request, including model downloads. Dictation keeps working.",
        ),
        kind: SettingKind::Bool,
        default: SettingValue::Bool(false),
        restart_required: false,
        visible: true,
        requires: None,
    },
    SettingSpec {
        key: keys::UPDATES_AUTO_CHECK,
        section: SettingSection::Updates,
        label: StaticStr::new("Check for updates"),
        help: StaticStr::new("Look for a newer version of Echo automatically."),
        kind: SettingKind::Bool,
        default: SettingValue::Bool(true),
        restart_required: false,
        visible: true,
        requires: Some(CapsRequirement::UpdaterAvailable),
    },
];
