/*!
 * SOURCE OF TRUTH KEYWORDS: SettingSpec, SettingKind, SettingValue, EnumOptions, OptionSource, SettingsSnapshot, SharedSettings, CapsRequirement, setting validation
 * WHAT:  The shape of a registry setting (SettingSpec with its SettingKind), the value a setting holds
 *        (SettingValue), where an Enum's options come from (EnumOptions / OptionSource), the conditions under which
 *        a setting or option is offered (CapsRequirement), the kind check every write passes
 *        (`SettingSpec::validate`), the resolved values the core reads (SettingsSnapshot) and the one live copy of
 *        them the running app shares (SharedSettings).
 * WHY:   One spec drives three things (02 §3.3): the settings service validates writes against `kind`, the
 *        Settings UI renders a control per `kind`, and src/lib/setting-schema.ts builds the Zod schema from it.
 *        Strings and lists are StaticStr/StaticList so registry entries are `const` while values arriving over IPC
 *        or resolved at runtime deserialize or build owned. Options that depend on what is installed (engines,
 *        their languages and accelerators) are an OptionSource, never a hardcoded list, so a new engine adapter
 *        shows up in Settings with no settings change. Visibility is a caps requirement, never a check on an
 *        engine or adapter name. `validate` checks only what the spec alone can prove; membership in runtime
 *        options is checked by registry/settings, which can resolve them.
 * WHERE: Entries in registry/settings; values stored by services/settings as JSON; sent to the UI by
 *        `registry_get`, `settings_get_all` and the SettingsChanged event; SettingsSnapshot is built by
 *        `registry::settings::resolve` and read by permission checks, the pipeline and commands.
 */

use std::{
    collections::{BTreeMap, HashSet},
    sync::Arc,
};

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use specta::Type;

use super::{AppError, SettingKey, StaticList, StaticStr};

/// The Settings page group a setting belongs to; equals the prefix of its key (`general.theme` → `general`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum SettingSection {
    General,
    Hotkeys,
    Session,
    Audio,
    Output,
    Transcription,
    Polish,
    Storage,
    Metrics,
    Privacy,
    Updates,
}

impl SettingSection {
    /// The key prefix of this section.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::General => "general",
            Self::Hotkeys => "hotkeys",
            Self::Session => "session",
            Self::Audio => "audio",
            Self::Output => "output",
            Self::Transcription => "transcription",
            Self::Polish => "polish",
            Self::Storage => "storage",
            Self::Metrics => "metrics",
            Self::Privacy => "privacy",
            Self::Updates => "updates",
        }
    }
}

/// A capability the active adapters must declare for a setting or option to be shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum CapsRequirement {
    /// The active ASR engine lists `gpu` in `AsrCaps.accelerators`.
    GpuAccelerator,
    /// The active ASR engine lists more than one language.
    MultipleLanguages,
    /// The hotkey adapter reports key-up (`HotkeyCaps.supports_release`), needed for hold-to-talk.
    HotkeyRelease,
    /// The updater adapter reports `UpdaterCaps.available`.
    UpdaterAvailable,
}

/// One choice of an `Enum` setting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct EnumOption {
    /// Stored value.
    pub value: StaticStr,
    pub label: StaticStr,
    /// The option is hidden unless the requirement holds.
    pub requires: Option<CapsRequirement>,
}

/// Where the options of an `Enum` setting come from when they depend on what is registered or installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum OptionSource {
    /// Every registered engine of kind `asr`.
    AsrEngines,
    /// Registered polisher engines that need a model (the opt-in LLM stage; the always-on rule stages need none).
    ModelPolishers,
    /// `auto` (when the selected ASR engine auto-detects) plus every language of that engine.
    AsrLanguages,
    /// `auto` plus every accelerator of the selected ASR engine.
    AsrAccelerators,
}

/// The options of an `Enum` setting: a fixed list, or a source resolved at runtime by the registry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "from", rename_all = "snake_case")]
pub enum EnumOptions {
    Fixed { list: StaticList<EnumOption> },
    Runtime { source: OptionSource },
}

/// The unit an `Int` setting is stored in, so the UI can label and format it without knowing the key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum SettingUnit {
    Milliseconds,
    Minutes,
    Days,
    WordsPerMinute,
}

/// One entry of a `Pairs` setting, e.g. a dictionary replacement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct TextPair {
    pub from: StaticStr,
    pub to: StaticStr,
}

/// The control and the validation rule of a setting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SettingKind {
    Bool,
    /// Inclusive range.
    Int {
        min: i32,
        max: i32,
        unit: Option<SettingUnit>,
    },
    Enum {
        options: EnumOptions,
    },
    /// A global shortcut accelerator such as `Ctrl+Alt+Space`; only the hotkey adapter parses it.
    Hotkey,
    /// An audio input device id; no value means the system default device. Its options are the devices the
    /// capture adapter lists at runtime (`audio_list_devices`).
    Device,
    Text {
        max_len: u32,
    },
    Pairs {
        max_pairs: u32,
        max_len: u32,
    },
}

impl SettingKind {
    /// Longest id-like value (enum value, shortcut, device id) a setting accepts, in characters.
    pub const MAX_TOKEN_LEN: usize = 128;

    /// The wire name of the kind, equal to the `kind` tag of its spec and of its values.
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Bool => "bool",
            Self::Int { .. } => "int",
            Self::Enum { .. } => "enum",
            Self::Hotkey => "hotkey",
            Self::Device => "device",
            Self::Text { .. } => "text",
            Self::Pairs { .. } => "pairs",
        }
    }
}

/// The value of a setting. The `kind` tag always equals the `kind` of the setting's spec.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum SettingValue {
    Bool(bool),
    Int(i32),
    Enum(StaticStr),
    Hotkey(StaticStr),
    Device(Option<StaticStr>),
    Text(StaticStr),
    Pairs(StaticList<TextPair>),
}

/// A registry setting entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct SettingSpec {
    pub key: SettingKey,
    pub section: SettingSection,
    pub label: StaticStr,
    pub help: StaticStr,
    pub kind: SettingKind,
    pub default: SettingValue,
    /// A change applies only after Echo restarts.
    pub restart_required: bool,
    /// Shown on the Settings page. False for internal state kept as a setting (e.g. onboarding completion).
    pub visible: bool,
    /// The setting is hidden unless the requirement holds.
    pub requires: Option<CapsRequirement>,
}

impl SettingSpec {
    /**
     * SOURCE OF TRUTH KEYWORDS: SettingSpec::validate, setting kind check, bounds check, settings write validation
     * WHAT:  Checks that `value` has this spec's kind and respects its bounds; the error names the setting key.
     * WHY:   The same rules back the Zod schema the UI builds from the spec, so Rust and the form agree. A runtime
     *        option list cannot be known here (types/ sees no registry), so a runtime Enum is only checked to be a
     *        well-formed token; registry::settings::validate adds the membership check. Caps requirements depend on
     *        the active adapters and are checked by the caller that holds them.
     * WHERE: registry::settings::{validate, resolve}; registry tests prove every default passes.
     */
    pub fn validate(&self, value: &SettingValue) -> Result<(), AppError> {
        let field = self.key.as_str();
        match (&self.kind, value) {
            (SettingKind::Bool, SettingValue::Bool(_)) => Ok(()),
            (SettingKind::Int { min, max, .. }, SettingValue::Int(number)) => {
                if (*min..=*max).contains(number) {
                    Ok(())
                } else {
                    Err(AppError::validation(
                        field,
                        format!("Choose a number from {min} to {max}."),
                    ))
                }
            }
            (SettingKind::Enum { options }, SettingValue::Enum(choice)) => match options {
                EnumOptions::Fixed { list } => {
                    if list.iter().any(|option| option.value == *choice) {
                        Ok(())
                    } else {
                        Err(AppError::validation(
                            field,
                            "Choose one of the listed options.",
                        ))
                    }
                }
                EnumOptions::Runtime { .. } => check_token(field, choice, "Choose an option."),
            },
            (SettingKind::Hotkey, SettingValue::Hotkey(shortcut)) => {
                check_token(field, shortcut, "Press a key combination.")
            }
            (SettingKind::Device, SettingValue::Device(device)) => match device {
                None => Ok(()),
                Some(id) => check_token(field, id, "Choose a microphone."),
            },
            (SettingKind::Text { max_len }, SettingValue::Text(text)) => {
                check_text(field, text, *max_len)
            }
            (SettingKind::Pairs { max_pairs, max_len }, SettingValue::Pairs(pairs)) => {
                check_pairs(field, pairs, *max_pairs, *max_len)
            }
            (kind, _) => Err(AppError::validation(
                field,
                format!("Expected a value of kind `{}`.", kind.name()),
            )),
        }
    }
}

/// A non-empty id-like value without control characters, at most `MAX_TOKEN_LEN` characters.
fn check_token(field: &str, text: &str, empty_message: &str) -> Result<(), AppError> {
    if text.trim().is_empty() {
        return Err(AppError::validation(field, empty_message));
    }
    if text.chars().count() > SettingKind::MAX_TOKEN_LEN || text.chars().any(char::is_control) {
        return Err(AppError::validation(field, "This value is not valid."));
    }
    Ok(())
}

/// Free text up to `max_len` characters; line breaks and tabs are the only control characters allowed.
fn check_text(field: &str, text: &str, max_len: u32) -> Result<(), AppError> {
    if exceeds(text, max_len) {
        return Err(AppError::validation(
            field,
            format!("Use at most {max_len} characters."),
        ));
    }
    if text
        .chars()
        .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
    {
        return Err(AppError::validation(
            field,
            "Remove unsupported characters.",
        ));
    }
    Ok(())
}

/// At most `max_pairs` pairs; each `from` is a unique (case-insensitive) non-empty word of at most `max_len`
/// characters, and each `to` fits `max_len` (empty `to` removes the word).
fn check_pairs(
    field: &str,
    pairs: &[TextPair],
    max_pairs: u32,
    max_len: u32,
) -> Result<(), AppError> {
    if pairs.len() > usize::try_from(max_pairs).unwrap_or(usize::MAX) {
        return Err(AppError::validation(
            field,
            format!("Use at most {max_pairs} entries."),
        ));
    }
    let mut seen = HashSet::with_capacity(pairs.len());
    for pair in pairs {
        if pair.from.trim().is_empty() {
            return Err(AppError::validation(
                field,
                "Every entry needs a word to replace.",
            ));
        }
        if exceeds(&pair.from, max_len) || exceeds(&pair.to, max_len) {
            return Err(AppError::validation(
                field,
                format!("Keep each entry to {max_len} characters or fewer."),
            ));
        }
        if pair
            .from
            .chars()
            .chain(pair.to.chars())
            .any(char::is_control)
        {
            return Err(AppError::validation(
                field,
                "Remove unsupported characters.",
            ));
        }
        if !seen.insert(pair.from.trim().to_lowercase()) {
            return Err(AppError::validation(
                field,
                format!("\"{}\" is listed more than once.", pair.from.trim()),
            ));
        }
    }
    Ok(())
}

fn exceeds(text: &str, max_len: u32) -> bool {
    text.chars().count() > usize::try_from(max_len).unwrap_or(usize::MAX)
}

/**
 * SOURCE OF TRUTH KEYWORDS: SettingsSnapshot, effective settings, resolved settings, stored over default, typed setting getters
 * WHAT:  The effective value of every setting (stored value over registry default), with typed getters.
 * WHY:   Rust owns settings state; the pipeline, permission checks and option resolution read one resolved view
 *        instead of each overlaying stored rows on defaults again. Getters return None for an unknown key or a
 *        kind mismatch, so a caller never reads a Bool as an Int by accident. Build it only through
 *        `registry::settings::resolve`, which drops stored values that no longer validate.
 * WHERE: Built by registry::settings; read by registry::permissions checks, registry option resolution, and later
 *        by the session actor and commands (refreshed after SettingsChanged).
 */
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SettingsSnapshot {
    values: BTreeMap<SettingKey, SettingValue>,
}

impl SettingsSnapshot {
    /// Wraps already-resolved values; `registry::settings::resolve` is the builder that validates them.
    pub fn from_resolved(values: impl IntoIterator<Item = (SettingKey, SettingValue)>) -> Self {
        Self {
            values: values.into_iter().collect(),
        }
    }

    pub fn get(&self, key: &SettingKey) -> Option<&SettingValue> {
        self.values.get(key)
    }

    pub fn bool(&self, key: &SettingKey) -> Option<bool> {
        match self.get(key)? {
            SettingValue::Bool(value) => Some(*value),
            _ => None,
        }
    }

    pub fn int(&self, key: &SettingKey) -> Option<i32> {
        match self.get(key)? {
            SettingValue::Int(value) => Some(*value),
            _ => None,
        }
    }

    pub fn enum_value(&self, key: &SettingKey) -> Option<&str> {
        match self.get(key)? {
            SettingValue::Enum(value) => Some(value.as_str()),
            _ => None,
        }
    }

    pub fn hotkey(&self, key: &SettingKey) -> Option<&str> {
        match self.get(key)? {
            SettingValue::Hotkey(value) => Some(value.as_str()),
            _ => None,
        }
    }

    /// The pinned device id: `Some(None)` means the system default device.
    pub fn device(&self, key: &SettingKey) -> Option<Option<&str>> {
        match self.get(key)? {
            SettingValue::Device(value) => Some(value.as_ref().map(StaticStr::as_str)),
            _ => None,
        }
    }

    pub fn pairs(&self, key: &SettingKey) -> Option<&[TextPair]> {
        match self.get(key)? {
            SettingValue::Pairs(pairs) => Some(pairs),
            _ => None,
        }
    }

    /// Every key and value, ordered by key.
    pub fn iter(&self) -> impl Iterator<Item = (&SettingKey, &SettingValue)> {
        self.values.iter()
    }
}

/**
 * SOURCE OF TRUTH KEYWORDS: SharedSettings, current settings, settings handle, replace snapshot, settings cache, RwLock Arc snapshot
 * WHAT:  The one live SettingsSnapshot of the running app: `current()` hands out the snapshot in effect, `replace`
 *        swaps in a newly resolved one. Clones share the same slot.
 * WHY:   Rust owns settings state (root CLAUDE.md §7), and the factory preflight, handlers and (later) the session
 *        actor must all read the same values without each querying the database. Readers get an `Arc` and drop
 *        the lock at once, so a slow reader never blocks a write and a snapshot never changes under a reader.
 *        Only the settings commands replace it, right after the settings service writes (step 06), so the cache
 *        and the table cannot drift.
 * WHERE: Built by app/bootstrap from `registry::settings::resolve`; held by ipc::CommandCtx (preflight via
 *        registry::permissions, handlers) and handed to the pipeline by app/.
 */
#[derive(Debug, Clone, Default)]
pub struct SharedSettings {
    slot: Arc<RwLock<Arc<SettingsSnapshot>>>,
}

impl SharedSettings {
    pub fn new(snapshot: SettingsSnapshot) -> Self {
        Self {
            slot: Arc::new(RwLock::new(Arc::new(snapshot))),
        }
    }

    /// The snapshot in effect now; later replacements do not change it.
    pub fn current(&self) -> Arc<SettingsSnapshot> {
        Arc::clone(&self.slot.read())
    }

    /// Makes `snapshot` the one every later `current()` returns.
    pub fn replace(&self, snapshot: SettingsSnapshot) {
        *self.slot.write() = Arc::new(snapshot);
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const THEMES: &[EnumOption] = &[
        EnumOption {
            value: StaticStr::new("system"),
            label: StaticStr::new("System"),
            requires: None,
        },
        EnumOption {
            value: StaticStr::new("dark"),
            label: StaticStr::new("Dark"),
            requires: None,
        },
    ];

    const THEME: SettingSpec = SettingSpec {
        key: SettingKey::from_static("general.theme"),
        section: SettingSection::General,
        label: StaticStr::new("Theme"),
        help: StaticStr::new("Follow Windows or pick light or dark."),
        kind: SettingKind::Enum {
            options: EnumOptions::Fixed {
                list: StaticList::new(THEMES),
            },
        },
        default: SettingValue::Enum(StaticStr::new("system")),
        restart_required: false,
        visible: true,
        requires: None,
    };

    fn spec(kind: SettingKind, default: SettingValue) -> SettingSpec {
        SettingSpec {
            key: SettingKey::from_static("test.value"),
            section: SettingSection::General,
            label: StaticStr::new("Test"),
            help: StaticStr::new("Test setting."),
            kind,
            default,
            restart_required: false,
            visible: true,
            requires: None,
        }
    }

    fn text(value: &str) -> StaticStr {
        StaticStr::from(value.to_owned())
    }

    fn pair(from: &str, to: &str) -> TextPair {
        TextPair {
            from: text(from),
            to: text(to),
        }
    }

    #[test]
    fn specs_can_be_declared_as_constants() {
        assert!(THEME.key.as_str().starts_with(THEME.section.as_str()));
        let value = serde_json::to_value(&THEME).unwrap();
        assert_eq!(value["kind"]["kind"], json!("enum"));
        assert_eq!(value["kind"]["options"]["from"], json!("fixed"));
        assert_eq!(value["visible"], json!(true));
        assert_eq!(
            value["default"],
            json!({ "kind": "enum", "value": "system" })
        );
        assert_eq!(serde_json::from_value::<SettingSpec>(value).unwrap(), THEME);
    }

    #[test]
    fn values_are_tagged_with_their_kind() {
        assert_eq!(
            serde_json::to_value(SettingValue::Int(3000)).unwrap(),
            json!({ "kind": "int", "value": 3000 })
        );
        assert_eq!(
            serde_json::to_value(SettingValue::Device(None)).unwrap(),
            json!({ "kind": "device", "value": null })
        );
        let pairs: SettingValue = serde_json::from_value(
            json!({ "kind": "pairs", "value": [{ "from": "echo", "to": "Echo" }] }),
        )
        .unwrap();
        assert_eq!(
            pairs,
            SettingValue::Pairs(StaticList::from(vec![pair("echo", "Echo")]))
        );
        assert!(
            serde_json::from_value::<SettingValue>(json!({ "kind": "int", "value": "3000" }))
                .is_err()
        );
    }

    #[test]
    fn kinds_serialize_their_limits_and_sources() {
        assert_eq!(
            serde_json::to_value(SettingKind::Int {
                min: 1000,
                max: 10_000,
                unit: Some(SettingUnit::Milliseconds),
            })
            .unwrap(),
            json!({ "kind": "int", "min": 1000, "max": 10000, "unit": "milliseconds" })
        );
        assert_eq!(
            serde_json::to_value(SettingKind::Hotkey).unwrap(),
            json!({ "kind": "hotkey" })
        );
        assert_eq!(
            serde_json::to_value(SettingKind::Enum {
                options: EnumOptions::Runtime {
                    source: OptionSource::AsrLanguages,
                },
            })
            .unwrap(),
            json!({ "kind": "enum", "options": { "from": "runtime", "source": "asr_languages" } })
        );
    }

    #[test]
    fn kind_names_equal_the_value_tags() {
        let pairs = [
            (SettingKind::Bool, SettingValue::Bool(true)),
            (
                SettingKind::Int {
                    min: 0,
                    max: 1,
                    unit: None,
                },
                SettingValue::Int(0),
            ),
            (THEME.kind.clone(), THEME.default.clone()),
            (SettingKind::Hotkey, SettingValue::Hotkey(text("Ctrl+A"))),
            (SettingKind::Device, SettingValue::Device(None)),
            (
                SettingKind::Text { max_len: 4 },
                SettingValue::Text(text("")),
            ),
            (
                SettingKind::Pairs {
                    max_pairs: 1,
                    max_len: 1,
                },
                SettingValue::Pairs(StaticList::new(&[])),
            ),
        ];
        for (kind, value) in pairs {
            assert_eq!(serde_json::to_value(&value).unwrap()["kind"], kind.name());
            assert_eq!(serde_json::to_value(&kind).unwrap()["kind"], kind.name());
        }
    }

    #[test]
    fn validate_rejects_a_value_of_another_kind() {
        let error = THEME.validate(&SettingValue::Bool(true)).unwrap_err();
        assert_eq!(
            error,
            AppError::validation("general.theme", "Expected a value of kind `enum`.")
        );
    }

    #[test]
    fn validate_checks_int_bounds_inclusively() {
        let countdown = spec(
            SettingKind::Int {
                min: 1000,
                max: 10_000,
                unit: Some(SettingUnit::Milliseconds),
            },
            SettingValue::Int(3000),
        );
        assert!(countdown.validate(&SettingValue::Int(1000)).is_ok());
        assert!(countdown.validate(&SettingValue::Int(10_000)).is_ok());
        assert!(countdown.validate(&SettingValue::Int(999)).is_err());
        assert!(countdown.validate(&SettingValue::Int(10_001)).is_err());
    }

    #[test]
    fn validate_checks_fixed_enum_membership_and_runtime_enum_shape() {
        assert!(THEME.validate(&SettingValue::Enum(text("dark"))).is_ok());
        assert!(THEME.validate(&SettingValue::Enum(text("sepia"))).is_err());

        let engine = spec(
            SettingKind::Enum {
                options: EnumOptions::Runtime {
                    source: OptionSource::AsrEngines,
                },
            },
            SettingValue::Enum(StaticStr::new("parakeet-tdt-0.6b-v3")),
        );
        assert!(
            engine
                .validate(&SettingValue::Enum(text("any-engine")))
                .is_ok()
        );
        assert!(engine.validate(&SettingValue::Enum(text(" "))).is_err());
        assert!(
            engine
                .validate(&SettingValue::Enum(text("a\u{0}b")))
                .is_err()
        );
        assert!(
            engine
                .validate(&SettingValue::Enum(text(&"x".repeat(129))))
                .is_err()
        );
    }

    #[test]
    fn validate_checks_hotkeys_and_devices() {
        let hotkey = spec(
            SettingKind::Hotkey,
            SettingValue::Hotkey(StaticStr::new("Ctrl+Alt+Space")),
        );
        assert!(hotkey.validate(&hotkey.default).is_ok());
        assert!(hotkey.validate(&SettingValue::Hotkey(text(""))).is_err());

        let device = spec(SettingKind::Device, SettingValue::Device(None));
        assert!(device.validate(&SettingValue::Device(None)).is_ok());
        assert!(
            device
                .validate(&SettingValue::Device(Some(text("usb-mic"))))
                .is_ok()
        );
        assert!(
            device
                .validate(&SettingValue::Device(Some(text(""))))
                .is_err()
        );
    }

    #[test]
    fn validate_checks_text_length_and_characters() {
        let note = spec(
            SettingKind::Text { max_len: 5 },
            SettingValue::Text(StaticStr::new("")),
        );
        assert!(note.validate(&SettingValue::Text(text("héllo"))).is_ok());
        assert!(note.validate(&SettingValue::Text(text("a\nb"))).is_ok());
        assert!(note.validate(&SettingValue::Text(text("toolong"))).is_err());
        assert!(note.validate(&SettingValue::Text(text("a\u{7}"))).is_err());
    }

    #[test]
    fn validate_checks_pairs() {
        let dictionary = spec(
            SettingKind::Pairs {
                max_pairs: 2,
                max_len: 10,
            },
            SettingValue::Pairs(StaticList::new(&[])),
        );
        let ok = |pairs: Vec<TextPair>| dictionary.validate(&SettingValue::Pairs(pairs.into()));
        assert!(ok(vec![pair("echo", "Echo"), pair("um", "")]).is_ok());
        assert!(ok(vec![pair("a", "b"), pair("c", "d"), pair("e", "f")]).is_err());
        assert!(ok(vec![pair("  ", "x")]).is_err());
        assert!(ok(vec![pair("echo", "Echo"), pair("ECHO ", "E")]).is_err());
        assert!(ok(vec![pair("echo", "a very long text")]).is_err());
        assert!(ok(vec![pair("ec\tho", "Echo")]).is_err());
    }

    #[test]
    fn snapshot_getters_are_typed() {
        let flag = SettingKey::from_static("privacy.offline_mode");
        let wpm = SettingKey::from_static("metrics.typing_wpm");
        let theme = SettingKey::from_static("general.theme");
        let device = SettingKey::from_static("audio.input_device");
        let snapshot = SettingsSnapshot::from_resolved([
            (flag.clone(), SettingValue::Bool(true)),
            (wpm.clone(), SettingValue::Int(40)),
            (theme.clone(), SettingValue::Enum(StaticStr::new("dark"))),
            (device.clone(), SettingValue::Device(None)),
        ]);
        assert_eq!(snapshot.bool(&flag), Some(true));
        assert_eq!(snapshot.int(&wpm), Some(40));
        assert_eq!(snapshot.enum_value(&theme), Some("dark"));
        assert_eq!(snapshot.device(&device), Some(None));
        assert_eq!(snapshot.int(&flag), None, "kind mismatch reads as None");
        assert_eq!(snapshot.bool(&SettingKey::from_static("missing.key")), None);
        assert_eq!(snapshot.iter().count(), 4);
    }

    #[test]
    fn shared_settings_hand_out_stable_snapshots_and_see_replacements() {
        let flag = SettingKey::from_static("privacy.offline_mode");
        let shared = SharedSettings::new(SettingsSnapshot::from_resolved([(
            flag.clone(),
            SettingValue::Bool(false),
        )]));
        let clone = shared.clone();
        let before = shared.current();
        clone.replace(SettingsSnapshot::from_resolved([(
            flag.clone(),
            SettingValue::Bool(true),
        )]));
        assert_eq!(
            before.bool(&flag),
            Some(false),
            "a held snapshot never changes"
        );
        assert_eq!(
            shared.current().bool(&flag),
            Some(true),
            "clones share one slot"
        );
        assert_eq!(SharedSettings::default().current().iter().count(), 0);
    }
}
