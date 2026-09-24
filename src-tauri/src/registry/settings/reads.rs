/*!
 * SOURCE OF TRUTH KEYWORDS: typed setting reads, theme read, transcription reads, polish reads, remove_fillers, dictionary, llm_polisher, trailing_space
 * WHAT:  Typed reads of a SettingsSnapshot for the settings the core acts on.
 * WHY:   Values are stored as tagged SettingValues and enum text; spelling them is the registry's job, so the
 *        pipeline asks here instead of matching kinds or comparing strings. A resolved snapshot always holds a
 *        valid value for every key, so each fallback only guards a snapshot built outside `resolve` and uses the
 *        spec's own default.
 * WHERE: Re-exported by registry/settings; read by pipeline/appearance, pipeline/asr and pipeline/polish.
 */

use super::{find, keys, values};
use crate::types::{
    Accelerator, EngineId, Language, SettingKey, SettingValue, SettingsSnapshot, TextPair,
    ThemePreference,
};

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
pub(super) const fn accelerator_value(accelerator: Accelerator) -> &'static str {
    match accelerator {
        Accelerator::Cpu => values::CPU,
        Accelerator::Gpu => values::GPU,
    }
}

/**
 * SOURCE OF TRUTH KEYWORDS: remove_fillers setting, dictionary setting, llm_polisher, trailing_space, polish settings read
 * WHAT:  Typed reads of the polish and output settings the polish chain acts on: whether fillers are removed, the
 *        dictionary pairs, the model polisher to run (None while grammar polish is off) and the trailing space.
 * WHY:   The chain is built from the registry and these reads (02 §8.3), so the pipeline never matches a
 *        SettingValue kind. The model polisher id is only a choice: whether it is registered and built is decided
 *        where the chain is built (pipeline/polish), which falls back to the rule output when it is not.
 * WHERE: pipeline/polish (chain plan and PolishContext).
 */
pub fn remove_fillers(settings: &SettingsSnapshot) -> bool {
    bool_or_default(settings, &keys::REMOVE_FILLERS)
}

/// `polish.dictionary`, in the order the user entered it.
pub fn dictionary(settings: &SettingsSnapshot) -> &[TextPair] {
    settings.pairs(&keys::DICTIONARY).unwrap_or_default()
}

/// The model polisher `polish.llm_engine` selects while `polish.llm_enabled` is on.
pub fn llm_polisher(settings: &SettingsSnapshot) -> Option<EngineId> {
    if !bool_or_default(settings, &keys::LLM_ENABLED) {
        return None;
    }
    settings
        .enum_value(&keys::LLM_ENGINE)
        .map(|id| EngineId::from(id.to_owned()))
}

/// `output.trailing_space`: end delivered text with a space so the next dictation does not glue on.
pub fn trailing_space(settings: &SettingsSnapshot) -> bool {
    bool_or_default(settings, &keys::TRAILING_SPACE)
}

/// A Bool setting, or its spec's default when the snapshot lacks it (only outside `resolve`).
fn bool_or_default(settings: &SettingsSnapshot, key: &SettingKey) -> bool {
    settings.bool(key).unwrap_or_else(|| {
        matches!(
            find(key).map(|spec| &spec.default),
            Some(SettingValue::Bool(true))
        )
    })
}
