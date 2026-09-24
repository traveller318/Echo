/*!
 * SOURCE OF TRUTH KEYWORDS: settings registry tests, setting defaults test, options test, validate test, typed reads test
 * WHAT:  Tests of the settings registry: the list, resolve, typed reads, runtime options and write validation.
 * WHY:   One file for the folder's tests, like pipeline/asr/tests.rs, so each production file stays small.
 * WHERE: `cargo test` (registry::settings::tests).
 */

use std::collections::HashSet;

use super::{
    list::THEME_OPTIONS,
    options::{accelerator_option, options_in, validate_in},
    reads::accelerator_value,
    *,
};
use crate::{
    registry::{engines, engines::tests::SAMPLE_ENGINES, hotkeys},
    types::{
        Accelerator, AppError, CapsRequirement, EnumOption, EnumOptions, Language, OptionSource,
        ResourceKind, SettingKey, SettingKind, SettingValue, SettingsSnapshot, StaticList,
        StaticStr, TextPair, ThemePreference,
    },
};

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
            let values: HashSet<&str> = list.iter().map(|option| option.value.as_str()).collect();
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

#[test]
fn polish_reads_follow_the_stored_values_and_their_defaults() {
    let defaults = defaults();
    assert!(remove_fillers(&defaults));
    assert!(trailing_space(&defaults));
    assert!(dictionary(&defaults).is_empty());
    assert_eq!(
        llm_polisher(&defaults),
        None,
        "grammar polish is off by default"
    );

    let pair = TextPair {
        from: text("cloud code"),
        to: text("Claude Code"),
    };
    let chosen = SettingsSnapshot::from_resolved([
        (keys::REMOVE_FILLERS, SettingValue::Bool(false)),
        (keys::TRAILING_SPACE, SettingValue::Bool(false)),
        (
            keys::DICTIONARY,
            SettingValue::Pairs(StaticList::from(vec![pair.clone()])),
        ),
        (keys::LLM_ENABLED, SettingValue::Bool(true)),
        (keys::LLM_ENGINE, SettingValue::Enum(text("sample-llm"))),
    ]);
    assert!(!remove_fillers(&chosen));
    assert!(!trailing_space(&chosen));
    assert_eq!(dictionary(&chosen), [pair]);
    assert_eq!(
        llm_polisher(&chosen),
        Some(crate::types::EngineId::from_static("sample-llm"))
    );

    let empty = SettingsSnapshot::default();
    assert!(
        remove_fillers(&empty),
        "a missing Bool falls back to its spec default"
    );
    assert!(dictionary(&empty).is_empty());
    assert_eq!(llm_polisher(&empty), None);
}
