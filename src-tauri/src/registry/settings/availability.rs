/*!
 * SOURCE OF TRUTH KEYWORDS: settings availability, requirement_holds, CapsRequirement evaluation, offered options, check_available, caps-gated setting
 * WHAT:  Decides what the Settings page may offer right now: whether a CapsRequirement holds (`requirement_holds`),
 *        the requirements that hold and every Enum setting's offered options (`availability`), and whether a write
 *        asks for something the running adapters cannot do (`check_available`).
 * WHY:   A setting or option is shown only when the active adapters declare the capability it needs (02 §3.4:
 *        the language picker only with more than one language, the accelerator only with a GPU, hold mode only
 *        with key-up, update controls only with an update source). The rule is evaluated here, from the selected
 *        engine's registry caps and the AdapterCaps the command layer passes in, never from an adapter name, and
 *        the UI and the write check read the same answer. Everything branches on caps; the one `match` over
 *        CapsRequirement lives in the registry, where feature branches belong (root CLAUDE.md §3).
 * WHERE: Re-exported by registry/settings; `settings_availability` (the view) and `settings_set` (the write check)
 *        in ipc/commands/settings.rs.
 */

use super::{
    SETTINGS,
    options::{options_in, selected_asr_caps},
};
use crate::{
    registry::engines::{self, EngineEntry},
    types::{
        Accelerator, AdapterCaps, AppError, CapsRequirement, EnumOption, EnumOptions, SettingKind,
        SettingOptions, SettingSpec, SettingValue, SettingsAvailability, SettingsSnapshot,
    },
};

/// Whether `requirement` holds for the engine `settings` select and the running adapters.
pub fn requirement_holds(
    requirement: CapsRequirement,
    settings: &SettingsSnapshot,
    adapters: &AdapterCaps,
) -> bool {
    requirement_holds_in(engines::ENGINES, requirement, settings, adapters)
}

/// What the Settings page may offer now: the requirements that hold and each Enum setting's offered options.
pub fn availability(settings: &SettingsSnapshot, adapters: &AdapterCaps) -> SettingsAvailability {
    availability_in(engines::ENGINES, settings, adapters)
}

/**
 * SOURCE OF TRUTH KEYWORDS: check_available, caps-gated write, unavailable setting, unavailable option, settings_set caps check
 * WHAT:  Refuses a write to a setting whose requirement does not hold, or of an Enum option that is not offered
 *        now (its own requirement fails); `Validation` on the setting's key.
 * WHY:   The UI never shows those, so such a write only comes from a stale page or another caller; storing it
 *        would make the core act on a capability it lacks (hold mode without key-up would never stop a take).
 *        Kind, bounds and runtime membership are `validate`'s; a reset to the default is always allowed.
 * WHERE: `settings_set`, after `validate`.
 */
pub fn check_available(
    spec: &SettingSpec,
    value: &SettingValue,
    settings: &SettingsSnapshot,
    adapters: &AdapterCaps,
) -> Result<(), AppError> {
    check_available_in(engines::ENGINES, spec, value, settings, adapters)
}

pub(super) fn requirement_holds_in(
    entries: &[EngineEntry],
    requirement: CapsRequirement,
    settings: &SettingsSnapshot,
    adapters: &AdapterCaps,
) -> bool {
    match requirement {
        CapsRequirement::GpuAccelerator => selected_asr_caps(entries, settings)
            .is_some_and(|caps| caps.supports_accelerator(Accelerator::Gpu)),
        CapsRequirement::MultipleLanguages => {
            selected_asr_caps(entries, settings).is_some_and(|caps| caps.languages.len() > 1)
        }
        CapsRequirement::HotkeyRelease => adapters.hotkeys.supports_release,
        CapsRequirement::UpdaterAvailable => adapters.updater.available,
    }
}

pub(super) fn availability_in(
    entries: &[EngineEntry],
    settings: &SettingsSnapshot,
    adapters: &AdapterCaps,
) -> SettingsAvailability {
    SettingsAvailability {
        caps: CapsRequirement::ALL
            .into_iter()
            .filter(|requirement| requirement_holds_in(entries, *requirement, settings, adapters))
            .collect(),
        options: SETTINGS
            .iter()
            .filter_map(|spec| {
                let SettingKind::Enum { options } = &spec.kind else {
                    return None;
                };
                Some(SettingOptions {
                    key: spec.key.clone(),
                    options: offered_in(entries, options, settings, adapters),
                })
            })
            .collect(),
    }
}

pub(super) fn check_available_in(
    entries: &[EngineEntry],
    spec: &SettingSpec,
    value: &SettingValue,
    settings: &SettingsSnapshot,
    adapters: &AdapterCaps,
) -> Result<(), AppError> {
    let field = spec.key.as_str();
    if let Some(requirement) = spec.requires
        && !requirement_holds_in(entries, requirement, settings, adapters)
    {
        return Err(AppError::validation(
            field,
            "This setting isn't available on this PC.",
        ));
    }
    if let (SettingKind::Enum { options }, SettingValue::Enum(choice)) = (&spec.kind, value)
        && !offered_in(entries, options, settings, adapters)
            .iter()
            .any(|option| option.value == *choice)
    {
        return Err(AppError::validation(
            field,
            "That option isn't available on this PC.",
        ));
    }
    Ok(())
}

/// The options offered now: a fixed list or a resolved runtime source, without those whose requirement fails.
fn offered_in(
    entries: &[EngineEntry],
    options: &EnumOptions,
    settings: &SettingsSnapshot,
    adapters: &AdapterCaps,
) -> Vec<EnumOption> {
    let all = match options {
        EnumOptions::Fixed { list } => list.to_vec(),
        EnumOptions::Runtime { source } => options_in(entries, *source, settings),
    };
    all.into_iter()
        .filter(|option| {
            option.requires.is_none_or(|requirement| {
                requirement_holds_in(entries, requirement, settings, adapters)
            })
        })
        .collect()
}
