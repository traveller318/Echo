/*!
 * SOURCE OF TRUTH KEYWORDS: settings registry, SETTINGS, setting keys, setting defaults, resolve settings, runtime options, validate setting, SettingsSnapshot
 * WHAT:  Every setting Echo has (02 §3.3) with its section, label, help, kind and bounds, default,
 *        restart_required and visibility; typed key constants; and the three operations on them: resolve stored
 *        values into a SettingsSnapshot, resolve an OptionSource into options, and validate a write.
 * WHY:   One list drives the settings service, the generated Settings UI and its Zod schema, so adding a setting
 *        is one entry here (root CLAUDE.md §7). Options that depend on what is installed (engines, their
 *        languages and accelerators) are OptionSources resolved from registry/engines at runtime, never a
 *        hardcoded list. Callers use the `keys` constants, never string literals, so a renamed key fails to
 *        compile instead of silently reading a default. Hotkey defaults come from registry/hotkeys so a hotkey
 *        and its setting cannot disagree.
 * WHERE: Read by services/settings callers (commands validate writes with `validate`), the pipeline (via
 *        `resolve` and the typed transcription reads), registry/permissions (offline mode), registry/hotkeys and
 *        `registry_get`.
 */

use super::{
    engines::{self, EngineEntry, EnginePort},
    hotkeys,
};
use crate::types::{
    Accelerator, AppError, AsrCaps, CapsRequirement, EngineId, EngineKind, EnumOption, EnumOptions,
    Language, OptionSource, ResourceKind, SettingKey, SettingKind, SettingSection, SettingSpec,
    SettingUnit, SettingValue, SettingsSnapshot, StaticList, StaticStr, ThemePreference,
};

/**
 * SOURCE OF TRUTH KEYWORDS: setting key constants, typed setting keys, keys module
 * WHAT:  One constant per setting key.
 * WHY:   Code that reads a setting names it through these, so a typo or a renamed key is a compile error.
 * WHERE: SETTINGS below; registry/permissions; later the pipeline and commands.
 */
pub mod keys {
    use crate::types::SettingKey;

    pub const LAUNCH_AT_STARTUP: SettingKey = SettingKey::from_static("general.launch_at_startup");
    pub const START_MINIMIZED: SettingKey = SettingKey::from_static("general.start_minimized");
    pub const SOUND_CUES: SettingKey = SettingKey::from_static("general.sound_cues");
    pub const THEME: SettingKey = SettingKey::from_static("general.theme");
    pub const RECORD_HOTKEY: SettingKey = SettingKey::from_static("hotkeys.record");
    pub const HOTKEY_MODE: SettingKey = SettingKey::from_static("hotkeys.mode");
    pub const PASTE_LAST_HOTKEY: SettingKey = SettingKey::from_static("hotkeys.paste_last");
    pub const CANCEL_COUNTDOWN_MS: SettingKey =
        SettingKey::from_static("session.cancel_countdown_ms");
    pub const MAX_DURATION_MIN: SettingKey = SettingKey::from_static("session.max_duration_min");
    pub const INPUT_DEVICE: SettingKey = SettingKey::from_static("audio.input_device");
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
}

/// Enum values the core compares against (the rest are only shown and stored).
pub mod values {
    /// `transcription.language` / `transcription.accelerator`: let the engine decide.
    pub const AUTO: &str = "auto";
    /// `hotkeys.mode`: press to start, press again to stop.
    pub const TOGGLE: &str = "toggle";
    /// `hotkeys.mode`: record while the keys are held.
    pub const HOLD: &str = "hold";
    /// `transcription.accelerator`: run on the processor.
    pub const CPU: &str = "cpu";
    /// `transcription.accelerator`: run on the graphics card (DirectML).
    pub const GPU: &str = "gpu";
}

const THEME_OPTIONS: &[EnumOption] = &[
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
        default: SettingValue::Enum(StaticStr::new(values::TOGGLE)),
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
        help: StaticStr::new("Days to keep transcripts. 0 keeps them forever."),
        kind: SettingKind::Int {
            min: 0,
            max: 3650,
            unit: Some(SettingUnit::Days),
        },
        default: SettingValue::Int(0),
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

/// The spec of `key`.
pub fn find(key: &SettingKey) -> Option<&'static SettingSpec> {
    SETTINGS.iter().find(|spec| spec.key == *key)
}

/**
 * SOURCE OF TRUTH KEYWORDS: resolve settings, stored over default, settings overlay, drop invalid stored value
 * WHAT:  Builds the SettingsSnapshot: every registry default, overlaid by each stored value that still passes its
 *        spec's kind check.
 * WHY:   Stored rows can outlive the spec that wrote them (a key removed, bounds tightened, a kind changed); such a
 *        value falls back to the default instead of reaching the pipeline. Unknown keys are ignored. Membership in
 *        runtime options is not checked here: an engine id whose adapter is gone is handled where the engine is
 *        built (NotFound → default engine), so resolving never needs the installed-engine state.
 * WHERE: Settings reads in commands and the session actor (after services/settings returns the stored rows);
 *        `defaults()`.
 */
pub fn resolve(stored: impl IntoIterator<Item = (SettingKey, SettingValue)>) -> SettingsSnapshot {
    let mut values: Vec<(SettingKey, SettingValue)> = SETTINGS
        .iter()
        .map(|spec| (spec.key.clone(), spec.default.clone()))
        .collect();
    for (key, value) in stored {
        let Some(spec) = find(&key) else { continue };
        if spec.validate(&value).is_err() {
            continue;
        }
        if let Some(slot) = values.iter_mut().find(|(existing, _)| *existing == key) {
            slot.1 = value;
        }
    }
    SettingsSnapshot::from_resolved(values)
}

/// Every setting at its default value.
pub fn defaults() -> SettingsSnapshot {
    resolve(std::iter::empty())
}

/// The `general.theme` choice in effect; a resolved snapshot always holds a valid one, so the default is only a
/// guard for a snapshot built outside `resolve`.
pub fn theme(settings: &SettingsSnapshot) -> ThemePreference {
    settings
        .enum_value(&keys::THEME)
        .and_then(ThemePreference::from_value)
        .unwrap_or_default()
}

/**
 * SOURCE OF TRUTH KEYWORDS: asr_engine setting, language_preference, accelerator_preference, transcription settings read
 * WHAT:  Typed reads of the transcription settings: the selected ASR engine id, the preferred language and the
 *        preferred accelerator, where `None` means `auto` (let the engine decide).
 * WHY:   The enum values are stored as text; spelling them (and the `auto` sentinel) is this registry's job, so the
 *        pipeline asks here instead of comparing strings. A preference is not yet a choice: the pipeline narrows it
 *        to what the loaded engine's caps offer (pipeline/asr/plan.rs), because the stored value was validated
 *        against the engine selected when it was written.
 * WHERE: pipeline/asr (the load request at startup and engine switch, the language of each take).
 */
pub fn asr_engine(settings: &SettingsSnapshot) -> Option<EngineId> {
    settings
        .enum_value(&keys::ASR_ENGINE)
        .map(|id| EngineId::from(id.to_owned()))
}

/// `transcription.language`; None for `auto` (or an unset value).
pub fn language_preference(settings: &SettingsSnapshot) -> Option<Language> {
    settings
        .enum_value(&keys::LANGUAGE)
        .filter(|value| *value != values::AUTO)
        .map(|value| Language::from(value.to_owned()))
}

/// `transcription.accelerator`; None for `auto` (or a value no accelerator is stored as).
pub fn accelerator_preference(settings: &SettingsSnapshot) -> Option<Accelerator> {
    let value = settings.enum_value(&keys::ACCELERATOR)?;
    [Accelerator::Cpu, Accelerator::Gpu]
        .into_iter()
        .find(|accelerator| accelerator_value(*accelerator) == value)
}

/// How an accelerator is stored in `transcription.accelerator`.
const fn accelerator_value(accelerator: Accelerator) -> &'static str {
    match accelerator {
        Accelerator::Cpu => values::CPU,
        Accelerator::Gpu => values::GPU,
    }
}

/// The options `source` offers right now, given the current settings (the selected ASR engine).
pub fn options(source: OptionSource, settings: &SettingsSnapshot) -> Vec<EnumOption> {
    options_in(engines::ENGINES, source, settings)
}

/**
 * SOURCE OF TRUTH KEYWORDS: validate setting write, runtime option membership, settings_set validation
 * WHAT:  Validates a write of `value` to `key`: the key exists, the value passes its spec's kind check and, for
 *        runtime options, is one of the options offered right now. Returns the spec on success.
 * WHY:   The command layer validates writes against the registry (02 §7.2); the settings service stays pure DB
 *        access. Caps requirements (e.g. hold mode needs key-up) are checked by the caller holding the adapters.
 * WHERE: `settings_set` (step 06) before services/settings writes the row.
 */
pub fn validate(
    key: &SettingKey,
    value: &SettingValue,
    settings: &SettingsSnapshot,
) -> Result<&'static SettingSpec, AppError> {
    validate_in(engines::ENGINES, key, value, settings)
}

fn validate_in(
    entries: &[EngineEntry],
    key: &SettingKey,
    value: &SettingValue,
    settings: &SettingsSnapshot,
) -> Result<&'static SettingSpec, AppError> {
    let spec = find(key).ok_or(AppError::NotFound {
        resource: ResourceKind::Setting,
    })?;
    spec.validate(value)?;
    if let (
        SettingKind::Enum {
            options: EnumOptions::Runtime { source },
        },
        SettingValue::Enum(choice),
    ) = (&spec.kind, value)
        && !options_in(entries, *source, settings)
            .iter()
            .any(|option| option.value == *choice)
    {
        return Err(AppError::validation(
            key.as_str(),
            "Choose one of the listed options.",
        ));
    }
    Ok(spec)
}

/**
 * SOURCE OF TRUTH KEYWORDS: OptionSource resolution, engine options, language options, accelerator options
 * WHAT:  Turns an OptionSource into options from an engine list: engines of a kind, or the languages and
 *        accelerators of the ASR engine `transcription.engine` selects (`auto` first where it applies).
 * WHY:   Branches on declared caps only (`auto_language`, `accelerators`, `needs_model`), never on an engine
 *        name. The public `options` passes ENGINES; tests pass sample entries. Language labels are the codes;
 *        the UI renders display names with `Intl.DisplayNames`. A selected engine that is not registered
 *        offers no languages or accelerators rather than guessing.
 * WHERE: `options`, `validate_in`.
 */
fn options_in(
    entries: &[EngineEntry],
    source: OptionSource,
    settings: &SettingsSnapshot,
) -> Vec<EnumOption> {
    match source {
        OptionSource::AsrEngines => entries
            .iter()
            .filter(|entry| entry.kind() == EngineKind::Asr)
            .map(engine_option)
            .collect(),
        OptionSource::ModelPolishers => entries
            .iter()
            .filter(|entry| {
                matches!(&entry.port, EnginePort::Polisher { caps, .. } if caps.needs_model)
            })
            .map(engine_option)
            .collect(),
        OptionSource::AsrLanguages => {
            let Some(caps) = selected_asr_caps(entries, settings) else {
                return Vec::new();
            };
            let auto = caps.auto_language.then(|| auto_option("Auto-detect"));
            auto.into_iter()
                .chain(caps.languages.iter().map(|language| EnumOption {
                    value: StaticStr::from(language.to_string()),
                    label: StaticStr::from(language.to_string()),
                    requires: None,
                }))
                .collect()
        }
        OptionSource::AsrAccelerators => {
            let Some(caps) = selected_asr_caps(entries, settings) else {
                return Vec::new();
            };
            std::iter::once(auto_option("Automatic"))
                .chain(caps.accelerators.iter().map(|accelerator| accelerator_option(*accelerator)))
                .collect()
        }
    }
}

fn selected_asr_caps<'a>(
    entries: &'a [EngineEntry],
    settings: &SettingsSnapshot,
) -> Option<&'a AsrCaps> {
    let selected = asr_engine(settings)?;
    entries
        .iter()
        .find(|entry| entry.id == selected)
        .and_then(EngineEntry::asr_caps)
}

fn engine_option(entry: &EngineEntry) -> EnumOption {
    EnumOption {
        value: StaticStr::from(entry.id.to_string()),
        label: entry.label.clone(),
        requires: None,
    }
}

fn auto_option(label: &'static str) -> EnumOption {
    EnumOption {
        value: StaticStr::new(values::AUTO),
        label: StaticStr::new(label),
        requires: None,
    }
}

fn accelerator_option(accelerator: Accelerator) -> EnumOption {
    let (label, requires) = match accelerator {
        Accelerator::Cpu => ("Processor (CPU)", None),
        Accelerator::Gpu => ("Graphics card (GPU)", Some(CapsRequirement::GpuAccelerator)),
    };
    let value = accelerator_value(accelerator);
    EnumOption {
        value: StaticStr::new(value),
        label: StaticStr::new(label),
        requires,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::registry::engines::tests::SAMPLE_ENGINES;
    use crate::types::TextPair;

    fn text(value: &str) -> StaticStr {
        StaticStr::from(value.to_owned())
    }

    fn selecting(engine: &str) -> SettingsSnapshot {
        resolve([(keys::ASR_ENGINE, SettingValue::Enum(text(engine)))])
    }

    fn values_of(options: &[EnumOption]) -> Vec<&str> {
        options.iter().map(|option| option.value.as_str()).collect()
    }

    #[test]
    fn theme_options_are_exactly_the_theme_preferences() {
        let values: Vec<&str> = THEME_OPTIONS
            .iter()
            .map(|option| option.value.as_str())
            .collect();
        let themes: Vec<&str> = ThemePreference::ALL
            .iter()
            .map(|theme| theme.as_str())
            .collect();
        assert_eq!(values, themes);
        assert_eq!(theme(&defaults()), ThemePreference::System);
        let dark = resolve([(keys::THEME, SettingValue::Enum(text("dark")))]);
        assert_eq!(theme(&dark), ThemePreference::Dark);
    }

    #[test]
    fn transcription_preferences_read_auto_as_none() {
        let defaults = defaults();
        assert_eq!(
            asr_engine(&defaults),
            Some(engines::PARAKEET_TDT_V3),
            "the default engine is registered"
        );
        assert_eq!(language_preference(&defaults), None);
        assert_eq!(accelerator_preference(&defaults), None);
        let chosen = resolve([
            (keys::LANGUAGE, SettingValue::Enum(text("de"))),
            (keys::ACCELERATOR, SettingValue::Enum(text("cpu"))),
        ]);
        assert_eq!(
            language_preference(&chosen),
            Some(Language::from_static("de"))
        );
        assert_eq!(accelerator_preference(&chosen), Some(Accelerator::Cpu));
        for accelerator in [Accelerator::Cpu, Accelerator::Gpu] {
            assert_eq!(
                accelerator_option(accelerator).value.as_str(),
                accelerator_value(accelerator)
            );
        }
    }

    #[test]
    fn every_default_validates_against_its_own_kind() {
        for spec in SETTINGS {
            assert_eq!(spec.validate(&spec.default), Ok(()), "{}", spec.key);
        }
    }

    #[test]
    fn keys_are_unique_and_prefixed_by_their_section() {
        let mut keys = HashSet::new();
        for spec in SETTINGS {
            let key = spec.key.as_str();
            assert!(keys.insert(key), "duplicate setting {key}");
            let (section, _) = key.split_once('.').unwrap_or_default();
            assert_eq!(section, spec.section.as_str(), "{key}");
            assert!(
                spec.key.is_well_formed(),
                "{key} is not section.snake_key (the settings commands would refuse it)"
            );
            assert!(!spec.label.trim().is_empty(), "{key} has no label");
            assert!(spec.help.ends_with('.'), "{key} help is not a sentence");
        }
    }

    #[test]
    fn every_documented_setting_is_registered() {
        let registered: Vec<&str> = SETTINGS.iter().map(|spec| spec.key.as_str()).collect();
        assert_eq!(
            registered,
            [
                "general.launch_at_startup",
                "general.start_minimized",
                "general.sound_cues",
                "general.theme",
                "hotkeys.record",
                "hotkeys.mode",
                "hotkeys.paste_last",
                "session.cancel_countdown_ms",
                "session.max_duration_min",
                "audio.input_device",
                "output.auto_paste",
                "output.keep_on_clipboard",
                "output.trailing_space",
                "transcription.engine",
                "transcription.language",
                "transcription.accelerator",
                "polish.remove_fillers",
                "polish.dictionary",
                "polish.llm_enabled",
                "polish.llm_engine",
                "storage.audio_retention_days",
                "storage.history_retention_days",
                "metrics.typing_wpm",
                "privacy.offline_mode",
                "updates.auto_check",
            ]
        );
    }

    #[test]
    fn sections_are_contiguous() {
        let mut finished = HashSet::new();
        let mut current = None;
        for spec in SETTINGS {
            if current != Some(spec.section) {
                assert!(
                    finished.insert(spec.section),
                    "{:?} is split across the list",
                    spec.section
                );
                current = Some(spec.section);
            }
        }
    }

    #[test]
    fn fixed_options_are_unique() {
        for spec in SETTINGS {
            if let SettingKind::Enum {
                options: EnumOptions::Fixed { list },
            } = &spec.kind
            {
                let values: HashSet<&str> =
                    list.iter().map(|option| option.value.as_str()).collect();
                assert_eq!(values.len(), list.len(), "{} repeats an option", spec.key);
            }
        }
    }

    #[test]
    fn engine_defaults_name_a_registered_engine_once_one_exists() {
        for (key, source) in [
            (keys::ASR_ENGINE, OptionSource::AsrEngines),
            (keys::LLM_ENGINE, OptionSource::ModelPolishers),
        ] {
            let offered = options(source, &defaults());
            if offered.is_empty() {
                continue;
            }
            let default = defaults().enum_value(&key).map(str::to_owned);
            assert!(
                offered
                    .iter()
                    .any(|option| Some(option.value.as_str()) == default.as_deref()),
                "{key} defaults to an engine that is not registered"
            );
        }
    }

    #[test]
    fn resolve_overlays_valid_stored_values_on_defaults() {
        let snapshot = resolve([
            (keys::TYPING_WPM, SettingValue::Int(80)),
            (keys::OFFLINE_MODE, SettingValue::Bool(true)),
            // Out of bounds, wrong kind and unknown key all fall back or are ignored.
            (keys::CANCEL_COUNTDOWN_MS, SettingValue::Int(50)),
            (keys::SOUND_CUES, SettingValue::Int(1)),
            (
                SettingKey::from_static("general.removed"),
                SettingValue::Bool(true),
            ),
        ]);
        assert_eq!(snapshot.int(&keys::TYPING_WPM), Some(80));
        assert_eq!(snapshot.bool(&keys::OFFLINE_MODE), Some(true));
        assert_eq!(snapshot.int(&keys::CANCEL_COUNTDOWN_MS), Some(3000));
        assert_eq!(snapshot.bool(&keys::SOUND_CUES), Some(true));
        assert_eq!(
            snapshot.get(&SettingKey::from_static("general.removed")),
            None
        );
        assert_eq!(snapshot.iter().count(), SETTINGS.len());
        assert_eq!(
            defaults().hotkey(&keys::RECORD_HOTKEY),
            Some(hotkeys::RECORD_DEFAULT)
        );
        assert_eq!(defaults().device(&keys::INPUT_DEVICE), Some(None));
        assert_eq!(defaults().pairs(&keys::DICTIONARY), Some(&[][..]));
    }

    #[test]
    fn engine_options_come_from_the_registry_by_kind_and_caps() {
        let settings = defaults();
        assert_eq!(
            values_of(&options_in(
                SAMPLE_ENGINES,
                OptionSource::AsrEngines,
                &settings
            )),
            ["sample-asr"]
        );
        assert_eq!(
            values_of(&options_in(
                SAMPLE_ENGINES,
                OptionSource::ModelPolishers,
                &settings
            )),
            ["sample-llm"],
            "the always-on rule polisher needs no model and is not selectable"
        );
    }

    #[test]
    fn language_and_accelerator_options_follow_the_selected_engine() {
        let settings = selecting("sample-asr");
        assert_eq!(
            values_of(&options_in(
                SAMPLE_ENGINES,
                OptionSource::AsrLanguages,
                &settings
            )),
            ["auto", "en", "de"]
        );
        let accelerators = options_in(SAMPLE_ENGINES, OptionSource::AsrAccelerators, &settings);
        assert_eq!(values_of(&accelerators), ["auto", "cpu", "gpu"]);
        assert_eq!(
            accelerators[2].requires,
            Some(CapsRequirement::GpuAccelerator)
        );

        let unknown = selecting("not-registered");
        assert!(options_in(SAMPLE_ENGINES, OptionSource::AsrLanguages, &unknown).is_empty());
        assert!(options_in(SAMPLE_ENGINES, OptionSource::AsrAccelerators, &unknown).is_empty());
    }

    #[test]
    fn validate_checks_the_key_the_kind_and_runtime_membership() {
        let settings = selecting("sample-asr");
        let check = |key: &SettingKey, value: SettingValue| {
            validate_in(SAMPLE_ENGINES, key, &value, &settings).map(|spec| spec.key.clone())
        };
        assert_eq!(
            check(&keys::LANGUAGE, SettingValue::Enum(text("de"))),
            Ok(keys::LANGUAGE)
        );
        assert_eq!(
            check(&keys::LANGUAGE, SettingValue::Enum(text("fr"))),
            Err(AppError::validation(
                "transcription.language",
                "Choose one of the listed options."
            ))
        );
        assert!(check(&keys::ASR_ENGINE, SettingValue::Enum(text("sample-llm"))).is_err());
        assert!(check(&keys::TYPING_WPM, SettingValue::Int(5)).is_err());
        assert_eq!(
            check(
                &SettingKey::from_static("general.missing"),
                SettingValue::Bool(true)
            ),
            Err(AppError::NotFound {
                resource: ResourceKind::Setting
            })
        );
        assert!(
            check(
                &keys::DICTIONARY,
                SettingValue::Pairs(StaticList::from(vec![TextPair {
                    from: text("echo"),
                    to: text("Echo"),
                }]))
            )
            .is_ok()
        );
        assert!(validate(&keys::THEME, &SettingValue::Enum(text("dark")), &settings).is_ok());
    }
}
