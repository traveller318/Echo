/*!
 * SOURCE OF TRUTH KEYWORDS: typed setting reads, sound_cues, notice_shown, onboarded, launch_at_startup, start_minimized, debug_log, theme read, transcription reads, polish reads, remove_fillers, dictionary, llm_polisher, trailing_space, delivery_policy, input_device, session_policy, record_mode, retention_policy, typing_wpm
 * WHAT:  Typed reads of a SettingsSnapshot for the settings the core acts on.
 * WHY:   Values are stored as tagged SettingValues and enum text; spelling them is the registry's job, so the
 *        pipeline asks here instead of matching kinds or comparing strings. A resolved snapshot always holds a
 *        valid value for every key, so each fallback only guards a snapshot built outside `resolve` and uses the
 *        spec's own default.
 * WHERE: Re-exported by registry/settings; read by pipeline/appearance, pipeline/asr, pipeline/polish,
 *        pipeline/delivery, pipeline/metrics (typing_wpm) and the session actor (session_policy, input_device,
 *        delivery_policy).
 */

use std::num::NonZeroU32;

use super::{find, keys, values};
use crate::types::{
    Accelerator, AudioDeviceId, DeliveryPolicy, EngineId, Language, OneTimeNotice, RecordMode,
    RetentionPolicy, SessionPolicy, SettingKey, SettingValue, SettingsSnapshot, TextPair,
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

/// `general.sound_cues`: play the start, stop, cancel and error chimes.
pub fn sound_cues(settings: &SettingsSnapshot) -> bool {
    bool_or_default(settings, &keys::SOUND_CUES)
}

/// `general.onboarded`: first-run onboarding was completed once.
pub fn onboarded(settings: &SettingsSnapshot) -> bool {
    bool_or_default(settings, &keys::ONBOARDED)
}

/// `general.launch_at_startup`: Echo registers itself to start at sign-in.
pub fn launch_at_startup(settings: &SettingsSnapshot) -> bool {
    bool_or_default(settings, &keys::LAUNCH_AT_STARTUP)
}

/// `general.start_minimized`: a start at sign-in keeps the main window hidden.
pub fn start_minimized(settings: &SettingsSnapshot) -> bool {
    bool_or_default(settings, &keys::START_MINIMIZED)
}

/// `general.debug_log`: the log files take debug lines too.
pub fn debug_log(settings: &SettingsSnapshot) -> bool {
    bool_or_default(settings, &keys::DEBUG_LOG)
}

/// The one-time `notice` was already shown (its hidden flag is set).
pub fn notice_shown(settings: &SettingsSnapshot, notice: &OneTimeNotice) -> bool {
    bool_or_default(settings, &notice.shown)
}

/// `audio.input_device`: the microphone the user pinned; None follows the Windows default input.
pub fn input_device(settings: &SettingsSnapshot) -> Option<AudioDeviceId> {
    settings
        .device(&keys::INPUT_DEVICE)
        .flatten()
        .map(|id| AudioDeviceId::from(id.to_owned()))
}

/// `hotkeys.mode`: whether the record hotkey toggles a take or records while held; a missing value is the default
/// (hold).
pub fn record_mode(settings: &SettingsSnapshot) -> RecordMode {
    match settings.enum_value(&keys::HOTKEY_MODE) {
        Some(values::TOGGLE) => RecordMode::Toggle,
        Some(values::HOLD) => RecordMode::Hold,
        _ => RecordMode::default(),
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

/**
 * SOURCE OF TRUTH KEYWORDS: retention_policy, audio retention setting, history retention setting, storage settings read
 * WHAT:  The RetentionPolicy `storage.audio_retention_days` and `storage.history_retention_days` give.
 * WHY:   The two zeros mean different things (audio 0 = delete right after success, history 0 = keep forever);
 *        RetentionPolicy spells them, so the pipeline never compares raw Ints. Read at every sweep and every settled
 *        take, so a change applies from the next one on. A negative value (only possible outside `resolve`) falls
 *        back to the spec default like every other read.
 * WHERE: pipeline/retention.rs (the sweeper, `release_after_success` for the session runner and retry, and the
 *        live re-sweep after a storage setting changes).
 */
pub fn retention_policy(settings: &SettingsSnapshot) -> RetentionPolicy {
    let days = |key: &SettingKey, fallback: u32| {
        int_or_default(settings, key)
            .and_then(|days| u32::try_from(days).ok())
            .unwrap_or(fallback)
    };
    let defaults = RetentionPolicy::DEFAULT;
    RetentionPolicy {
        audio_days: days(&keys::AUDIO_RETENTION_DAYS, defaults.audio_days),
        history_days: days(&keys::HISTORY_RETENTION_DAYS, defaults.history_days),
    }
}

/**
 * SOURCE OF TRUTH KEYWORDS: typing_wpm, typing speed setting, time saved input, metrics settings read
 * WHAT:  `metrics.typing_wpm`: the user's typing speed in words per minute, never zero.
 * WHY:   Time saved divides by it (02 §7.4). The registry bounds it to its Int range, so the fallbacks only guard a
 *        snapshot built outside `resolve`: a value that is not a positive number falls back to the spec default,
 *        and the type rules out a division by zero instead of a check at the formula.
 * WHERE: pipeline/metrics (the time-saved formula), read at every metrics_summary, so a new speed applies at once.
 */
pub fn typing_wpm(settings: &SettingsSnapshot) -> NonZeroU32 {
    let positive = |value: i32| u32::try_from(value).ok().and_then(NonZeroU32::new);
    settings
        .int(&keys::TYPING_WPM)
        .and_then(positive)
        .or_else(|| int_default(&keys::TYPING_WPM).and_then(positive))
        .unwrap_or(NonZeroU32::MIN)
}

/// An Int setting, or its spec's default when the snapshot lacks it (only outside `resolve`).
fn int_or_default(settings: &SettingsSnapshot, key: &SettingKey) -> Option<i32> {
    settings.int(key).or_else(|| int_default(key))
}

/// The registry default of an Int setting.
fn int_default(key: &SettingKey) -> Option<i32> {
    match find(key).map(|spec| &spec.default) {
        Some(SettingValue::Int(value)) => Some(*value),
        _ => None,
    }
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
