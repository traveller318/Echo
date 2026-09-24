/*!
 * SOURCE OF TRUTH KEYWORDS: caps structs, AsrCaps, AudioCaps, VadCaps, PolisherCaps, HotkeyCaps, InserterCaps, UpdaterCaps, EngineCaps, EngineSpec, Accelerator
 * WHAT:  The capability struct of every port that declares caps (02 §3.4), plus the vocabulary they use:
 *        Accelerator, Language, LanguageSupport, LatencyClass and EngineKind. EngineCaps / EngineSpec are the
 *        IPC view of a registry engine entry (id, label, model, caps tagged by kind).
 * WHY:   The core branches on what an adapter declares, never on its name, so swapping a model or OS integration
 *        is an adapter plus a registry entry (00 constraint 4). Caps are types, so they live here and the port
 *        traits import them. Lists are `StaticList<T>`: a registry entry declares them as `const` borrowed
 *        slices, and an adapter that discovers caps at runtime can still hand back an owned list.
 *        EngineSpec carries no build fn (that needs ports, which types/ cannot import); registry/engines keeps
 *        the constructor next to the same caps and derives this view, so the two cannot disagree.
 * WHERE: Declared by adapters and registry/engines entries; read by pipeline (e.g. skip rule casing when
 *        `AsrCaps.casing`) and by the Models/Settings UI through the generated bindings.
 */

use serde::{Deserialize, Serialize};
use specta::Type;

use super::{
    EngineId, ModelId, PIPELINE_SAMPLE_RATE_HZ, StaticList, StaticStr, ids::static_str_id,
};

static_str_id! {
    /// A spoken-language code (ISO 639-1, e.g. `en`, `de`). "Auto-detect" is not a language; it is the absence
    /// of one (`Option<Language>`), offered only when `AsrCaps.auto_language` is set.
    Language
}

/// What an engine registry entry builds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum EngineKind {
    Asr,
    Polisher,
    Vad,
}

/// Where inference runs. `gpu` means any DX12 adapter (DirectML), not one vendor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum Accelerator {
    Cpu,
    Gpu,
}

/// How much latency a polisher adds to delivery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum LatencyClass {
    /// Milliseconds; always safe on the delivery path.
    Instant,
    /// Hundreds of milliseconds; must run under a timeout with a fallback (02 §8.3).
    Slow,
}

/// Which spoken languages a component handles.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LanguageSupport {
    Any,
    Only { languages: StaticList<Language> },
}

impl LanguageSupport {
    pub fn supports(&self, language: &Language) -> bool {
        match self {
            Self::Any => true,
            Self::Only { languages } => languages.contains(language),
        }
    }
}

/// Caps of an `AudioCapture` adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct AudioCaps {
    /// Device sample rates the adapter can open, in Hz.
    pub sample_rates: StaticList<u32>,
    /// Channel counts the adapter can open.
    pub channels: StaticList<u16>,
}

/// Caps of a `VoiceActivity` adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct VadCaps {
    /// Length of the frame `push` expects, in milliseconds of 16 kHz audio.
    pub frame_ms: u32,
}

impl VadCaps {
    /// Samples in one frame at the pipeline rate, e.g. 32 ms → 512 (the exact frame Silero takes, 05 A11).
    pub const fn frame_samples(self) -> usize {
        // Widening u32 → u64 → usize on the 64-bit Windows target; the product cannot overflow u64.
        (self.frame_ms as u64 * PIPELINE_SAMPLE_RATE_HZ as u64 / 1000) as usize
    }
}

/// Caps of an `AsrEngine` adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct AsrCaps {
    pub languages: StaticList<Language>,
    /// The engine detects the spoken language itself.
    pub auto_language: bool,
    /// Output already carries punctuation.
    pub punctuation: bool,
    /// Output already carries sentence casing, so the rule polisher skips its casing stage.
    pub casing: bool,
    pub accelerators: StaticList<Accelerator>,
    /// Longest audio segment the engine accepts in one `transcribe` call, in seconds.
    pub max_segment_s: u32,
}

impl AsrCaps {
    pub fn supports_accelerator(&self, accelerator: Accelerator) -> bool {
        self.accelerators.contains(&accelerator)
    }
}

/// Caps of a `TextPolisher` adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct PolisherCaps {
    pub latency_class: LatencyClass,
    pub languages: LanguageSupport,
    /// A model must be installed before the polisher can run.
    pub needs_model: bool,
}

/// Caps of a `HotkeyService` adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct HotkeyCaps {
    /// Key-up events are reported, so hold-to-talk is possible.
    pub supports_release: bool,
    /// A combination of modifiers alone (e.g. Ctrl+Alt) can be bound.
    pub supports_modifier_only: bool,
}

/// Caps of a `TextInserter` adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct InserterCaps {
    /// Can paste into windows running at a higher integrity level than Echo (05 W2).
    pub can_target_elevated: bool,
    /// Inserts by pasting what the clipboard holds, so delivery writes the text to the clipboard first.
    pub uses_clipboard: bool,
}

/// Caps of an `Updater` adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct UpdaterCaps {
    /// Updates can be checked and installed; false hides every update control (02 §11).
    pub available: bool,
}

/// The caps a registry engine entry declares, tagged by the kind of engine it builds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EngineCaps {
    Asr(AsrCaps),
    Polisher(PolisherCaps),
    Vad(VadCaps),
}

impl EngineCaps {
    pub const fn kind(&self) -> EngineKind {
        match self {
            Self::Asr(_) => EngineKind::Asr,
            Self::Polisher(_) => EngineKind::Polisher,
            Self::Vad(_) => EngineKind::Vad,
        }
    }
}

/// What the UI (Models page, Settings options) knows about a registry engine entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct EngineSpec {
    pub id: EngineId,
    pub label: StaticStr,
    /// The model manifest the engine runs, when it needs one.
    pub model_id: Option<ModelId>,
    pub caps: EngineCaps,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const ENGLISH: Language = Language::from_static("en");
    const GERMAN: Language = Language::from_static("de");

    #[test]
    fn caps_can_be_declared_as_constants() {
        const LANGUAGES: &[Language] = &[ENGLISH, GERMAN];
        const CAPS: AsrCaps = AsrCaps {
            languages: StaticList::new(LANGUAGES),
            auto_language: true,
            punctuation: true,
            casing: true,
            accelerators: StaticList::new(&[Accelerator::Cpu, Accelerator::Gpu]),
            max_segment_s: 20,
        };
        assert!(CAPS.supports_accelerator(Accelerator::Gpu));
        assert_eq!(
            serde_json::to_value(&CAPS).unwrap(),
            json!({
                "languages": ["en", "de"],
                "auto_language": true,
                "punctuation": true,
                "casing": true,
                "accelerators": ["cpu", "gpu"],
                "max_segment_s": 20,
            })
        );
    }

    #[test]
    fn engine_caps_are_tagged_by_kind() {
        let spec = EngineSpec {
            id: EngineId::from_static("silero-vad-v5"),
            label: StaticStr::new("Silero VAD"),
            model_id: None,
            caps: EngineCaps::Vad(VadCaps { frame_ms: 32 }),
        };
        assert_eq!(spec.caps.kind(), EngineKind::Vad);
        assert_eq!(
            serde_json::to_value(&spec).unwrap(),
            json!({
                "id": "silero-vad-v5",
                "label": "Silero VAD",
                "model_id": null,
                "caps": { "kind": "vad", "frame_ms": 32 },
            })
        );
    }

    #[test]
    fn vad_frame_samples_follow_the_pipeline_rate() {
        assert_eq!(VadCaps { frame_ms: 32 }.frame_samples(), 512);
        assert_eq!(VadCaps { frame_ms: 30 }.frame_samples(), 480);
    }

    #[test]
    fn language_support_checks_membership() {
        const ENGLISH_ONLY: &[Language] = &[ENGLISH];
        const ONLY_ENGLISH: LanguageSupport = LanguageSupport::Only {
            languages: StaticList::new(ENGLISH_ONLY),
        };
        assert!(ONLY_ENGLISH.supports(&ENGLISH));
        assert!(!ONLY_ENGLISH.supports(&GERMAN));
        assert!(LanguageSupport::Any.supports(&GERMAN));
        assert_eq!(
            serde_json::to_value(LanguageSupport::Any).unwrap(),
            json!({ "kind": "any" })
        );
        assert_eq!(
            serde_json::to_value(&ONLY_ENGLISH).unwrap(),
            json!({ "kind": "only", "languages": ["en"] })
        );
    }
}
