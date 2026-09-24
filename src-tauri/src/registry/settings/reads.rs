/*!
 * SOURCE OF TRUTH KEYWORDS: typed setting reads, theme read, transcription reads, polish reads, remove_fillers, dictionary, llm_polisher, trailing_space, delivery_policy, session_policy, record_mode
 * WHAT:  Typed reads of a SettingsSnapshot for the settings the core acts on.
 * WHY:   Values are stored as tagged SettingValues and enum text; spelling them is the registry's job, so the
 *        pipeline asks here instead of matching kinds or comparing strings. A resolved snapshot always holds a
 *        valid value for every key, so each fallback only guards a snapshot built outside `resolve` and uses the
 *        spec's own default.
 * WHERE: Re-exported by registry/settings; read by pipeline/appearance, pipeline/asr, pipeline/polish,
 *        pipeline/delivery and the session actor (session_policy).
 */

use super::{find, keys, values};
use crate::types::{
    Accelerator, DeliveryPolicy, EngineId, Language, RecordMode, SessionPolicy, SettingKey,
    SettingValue, SettingsSnapshot, TextPair, ThemePreference,
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

/**
 * SOURCE OF TRUTH KEYWORDS: delivery_policy, auto_paste setting, keep_on_clipboard setting, output settings read
 * WHAT:  The DeliveryPolicy the `output.auto_paste` and `output.keep_on_clipboard` settings give.
 * WHY:   Delivery decides from one typed policy, read when the take is delivered, so a toggle changed during a take
 *        applies to that take's delivery.
 * WHERE: pipeline/delivery.rs callers (the session actor, paste-last).
 */
pub fn delivery_policy(settings: &SettingsSnapshot) -> DeliveryPolicy {
    DeliveryPolicy {
        auto_paste: bool_or_default(settings, &keys::AUTO_PASTE),
        keep_on_clipboard: bool_or_default(settings, &keys::KEEP_ON_CLIPBOARD),
    }
}

/// `hotkeys.mode`: whether the record hotkey toggles a take or records while held.
pub fn record_mode(settings: &SettingsSnapshot) -> RecordMode {
    match settings.enum_value(&keys::HOTKEY_MODE) {
        Some(values::HOLD) => RecordMode::Hold,
        _ => RecordMode::Toggle,
    }
}

/**
 * SOURCE OF TRUTH KEYWORDS: session_policy, cancel countdown setting, max duration setting, hotkey mode setting, take rules read
 * WHAT:  The SessionPolicy a take started now runs under: SessionPolicy::DEFAULT's fixed rules with the record mode,
 *        the Esc countdown and the longest take taken from settings.
 * WHY:   The session actor reads this once per record press and the state machine copies it into the take, so a
 *        setting changed mid-take applies from the next take on. `session.max_duration_min` is stored in minutes
 *        and the machine counts milliseconds; a negative value (only possible outside `resolve`) falls back to the
 *        spec default like every other read.
 * WHERE: The session actor (step 14) builds SessionInput::RecordPressed with it.
 */
pub fn session_policy(settings: &SettingsSnapshot) -> SessionPolicy {
    let defaults = SessionPolicy::DEFAULT;
    let cancel_countdown_ms = int_or_default(settings, &keys::CANCEL_COUNTDOWN_MS)
        .and_then(|millis| u32::try_from(millis).ok())
        .unwrap_or(defaults.cancel_countdown_ms);
    let max_duration_ms = int_or_default(settings, &keys::MAX_DURATION_MIN)
        .and_then(|minutes| u64::try_from(minutes).ok())
        .map_or(defaults.max_duration_ms, |minutes| {
            minutes.saturating_mul(MS_PER_MINUTE)
        });
    SessionPolicy {
        record_mode: record_mode(settings),
        cancel_countdown_ms,
        max_duration_ms,
        ..defaults
    }
}

const MS_PER_MINUTE: u64 = 60_000;

/// An Int setting, or its spec's default when the snapshot lacks it (only outside `resolve`).
fn int_or_default(settings: &SettingsSnapshot, key: &SettingKey) -> Option<i32> {
    settings
        .int(key)
        .or_else(|| match find(key).map(|spec| &spec.default) {
            Some(SettingValue::Int(value)) => Some(*value),
            _ => None,
        })
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
