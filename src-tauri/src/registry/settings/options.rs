/*!
 * SOURCE OF TRUTH KEYWORDS: setting options, validate setting write, OptionSource resolution, runtime option membership
 * WHAT:  `options` (what an OptionSource offers right now) and `validate` (is a write allowed), over the engine list.
 * WHY:   Runtime options come from registry/engines caps, never a hardcoded list; a write is checked against the
 *        same options the Settings UI shows.
 * WHERE: Re-exported by registry/settings; `settings_set`, `registry_get` and the generated Settings UI.
 */

use super::{find, reads::accelerator_value, reads::asr_engine, values};
use crate::{
    registry::engines::{self, EngineEntry},
    types::{
        Accelerator, AppError, AsrCaps, CapsRequirement, EngineKind, EnumOption, EnumOptions,
        OptionSource, ResourceKind, SettingKey, SettingKind, SettingSpec, SettingValue,
        SettingsSnapshot, StaticStr,
    },
};

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

pub(super) fn validate_in(
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
pub(super) fn options_in(
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
            .filter(|entry| entry.is_model_polisher())
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
                .chain(
                    caps.accelerators
                        .iter()
                        .map(|accelerator| accelerator_option(*accelerator)),
                )
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

pub(super) fn accelerator_option(accelerator: Accelerator) -> EnumOption {
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
