/*!
 * SOURCE OF TRUTH KEYWORDS: engine registry, EngineEntry, EnginePort, BuildCtx, build_asr, build_polisher, build_vad, SILERO_VAD, default_vad, lazy adapter construction
 * WHAT:  The list of every local AI engine (ASR, polisher, VAD): id, label, model, declared caps and a lazy
 *        `build` fn; lookups by id and kind; the IPC view (EngineSpec); and the typed builders the composition
 *        root and pipeline call. BuildCtx is what a build fn may use.
 * WHY:   Adding an engine is an adapter plus one entry here (02 §8.4); nothing else names a model. Caps and the
 *        constructor sit in one EnginePort variant, so an entry cannot declare ASR caps and build a polisher.
 *        Only the selected engine is ever constructed: `build` runs when bootstrap or an engine switch asks
 *        for that id, never at startup for the whole list (02 §3.5). VAD builds a fresh `Box` because detectors
 *        are stateful per stream (05 A11). BuildCtx lives here, not in types/, because it holds a port handle
 *        and types/ may not import ports (02 §3.2). Concrete entries arrive with their adapters: Silero VAD
 *        (step 09) is here; Parakeet (step 10), rules (step 11) and Qwen3 (step 23) follow.
 * WHERE: Built through by app/bootstrap and pipeline/models.rs (engine switch); read by registry/settings
 *        (runtime options), `registry_get` and the Models page (via `specs`).
 */

use std::sync::Arc;

use super::models;
use crate::{
    adapters::vad::SileroVad,
    ports::{AsrEngine, ModelStore, TextPolisher, VoiceActivity},
    types::{
        AppError, AppPaths, AsrCaps, EngineCaps, EngineId, EngineKind, EngineSpec, ModelId,
        ModelManifest, PolisherCaps, PortError, PortResult, ResourceKind, StaticStr, VadCaps,
    },
};

/// Everything an engine's build fn may use.
pub struct BuildCtx {
    /// Data and resource locations (bundled Silero model, ONNX Runtime DLLs, `models/`, `runtimes/`).
    pub paths: AppPaths,
    /// Finds installed model files; engines that load lazily keep it to locate their model later.
    pub model_store: Arc<dyn ModelStore>,
}

pub type BuildAsr = fn(&BuildCtx) -> PortResult<Arc<dyn AsrEngine>>;
pub type BuildPolisher = fn(&BuildCtx) -> PortResult<Arc<dyn TextPolisher>>;
pub type BuildVad = fn(&BuildCtx) -> PortResult<Box<dyn VoiceActivity>>;

/// The port an engine implements, the caps it declares and its lazy constructor.
pub enum EnginePort {
    Asr {
        caps: AsrCaps,
        build: BuildAsr,
    },
    Polisher {
        caps: PolisherCaps,
        build: BuildPolisher,
    },
    Vad {
        caps: VadCaps,
        build: BuildVad,
    },
}

/// A registry engine entry.
pub struct EngineEntry {
    /// Kebab-case, e.g. `parakeet-tdt-0.6b-v3`; stored by the engine settings.
    pub id: EngineId,
    pub label: StaticStr,
    /// The model manifest the engine runs (registry/models), when it needs one.
    pub model_id: Option<ModelId>,
    pub port: EnginePort,
}

impl EngineEntry {
    pub const fn kind(&self) -> EngineKind {
        match self.port {
            EnginePort::Asr { .. } => EngineKind::Asr,
            EnginePort::Polisher { .. } => EngineKind::Polisher,
            EnginePort::Vad { .. } => EngineKind::Vad,
        }
    }

    /// The caps as the UI sees them.
    pub fn caps(&self) -> EngineCaps {
        match &self.port {
            EnginePort::Asr { caps, .. } => EngineCaps::Asr(caps.clone()),
            EnginePort::Polisher { caps, .. } => EngineCaps::Polisher(caps.clone()),
            EnginePort::Vad { caps, .. } => EngineCaps::Vad(*caps),
        }
    }

    /// The IPC view of this entry.
    pub fn spec(&self) -> EngineSpec {
        EngineSpec {
            id: self.id.clone(),
            label: self.label.clone(),
            model_id: self.model_id.clone(),
            caps: self.caps(),
        }
    }

    /// The manifest of the model this engine runs.
    pub fn manifest(&self) -> Option<&'static ModelManifest> {
        self.model_id.as_ref().and_then(models::find)
    }
}

/// Registry id of the Silero VAD v5 detector.
pub const SILERO_VAD: EngineId = EngineId::from_static("silero-vad-v5");

/// Every engine, in the order the Models page lists them.
pub const ENGINES: &[EngineEntry] = &[EngineEntry {
    id: SILERO_VAD,
    label: StaticStr::new("Silero VAD"),
    model_id: Some(models::SILERO_VAD_V5),
    port: EnginePort::Vad {
        caps: SileroVad::CAPS,
        build: build_silero_vad,
    },
}];

/// The engine with `id`.
pub fn find(id: &EngineId) -> Option<&'static EngineEntry> {
    ENGINES.iter().find(|entry| entry.id == *id)
}

/**
 * SOURCE OF TRUTH KEYWORDS: default_vad, voice activity engine choice, first VAD entry, build_default_vad
 * WHAT:  The detector a take uses: the first VAD entry in ENGINES; `build_default_vad` builds a fresh one.
 * WHY:   Voice activity is not a user setting, so the choice is registry order; the session asks here instead of
 *        naming an engine (root CLAUDE.md §3). Adding a better detector first in ENGINES switches every take.
 * WHERE: The session actor (a detector per take, reused across takes) and tests.
 */
pub fn default_vad() -> Option<&'static EngineEntry> {
    of_kind(EngineKind::Vad).next()
}

/// Builds a fresh detector from the default VAD engine; `NotFound { engine }` when none is registered.
pub fn build_default_vad(ctx: &BuildCtx) -> PortResult<Box<dyn VoiceActivity>> {
    match default_vad() {
        Some(entry) => build_vad(&entry.id, ctx),
        None => Err(PortError::new(AppError::NotFound {
            resource: ResourceKind::Engine,
        })
        .with_detail("no VAD engine is registered")),
    }
}

/// Silero VAD v5 on the bundled model (05 A11).
fn build_silero_vad(ctx: &BuildCtx) -> PortResult<Box<dyn VoiceActivity>> {
    let model = models::bundled_file(&ctx.paths, &models::SILERO_VAD_V5)?;
    Ok(Box::new(SileroVad::load(&ctx.paths, &model)?))
}

/// Every engine of `kind`, in registry order.
pub fn of_kind(kind: EngineKind) -> impl Iterator<Item = &'static EngineEntry> {
    ENGINES.iter().filter(move |entry| entry.kind() == kind)
}

/// The IPC view of every engine.
pub fn specs() -> Vec<EngineSpec> {
    ENGINES.iter().map(EngineEntry::spec).collect()
}

/// Builds the ASR engine `id`. Fails with `NotFound { engine }` when no ASR engine has that id.
pub fn build_asr(id: &EngineId, ctx: &BuildCtx) -> PortResult<Arc<dyn AsrEngine>> {
    build_asr_in(ENGINES, id, ctx)
}

/// Builds the polisher `id`. Fails with `NotFound { engine }` when no polisher has that id.
pub fn build_polisher(id: &EngineId, ctx: &BuildCtx) -> PortResult<Arc<dyn TextPolisher>> {
    build_polisher_in(ENGINES, id, ctx)
}

/// Builds a fresh detector from the VAD engine `id`. Fails with `NotFound { engine }` when no VAD has that id.
pub fn build_vad(id: &EngineId, ctx: &BuildCtx) -> PortResult<Box<dyn VoiceActivity>> {
    build_vad_in(ENGINES, id, ctx)
}

/**
 * SOURCE OF TRUTH KEYWORDS: typed engine build, engine kind mismatch, engine not found
 * WHAT:  The builders over an explicit entry list: find `id`, check it builds the requested port, run its build.
 * WHY:   The public builders pass ENGINES; tests pass sample entries backed by the port fakes, so the lookup and
 *        kind checks are exercised before any real adapter exists. A wrong kind is `NotFound`, the same as an
 *        unknown id: to the caller, no engine of that kind has the id.
 * WHERE: build_asr / build_polisher / build_vad above; tests below.
 */
fn build_asr_in(
    entries: &[EngineEntry],
    id: &EngineId,
    ctx: &BuildCtx,
) -> PortResult<Arc<dyn AsrEngine>> {
    match entry_in(entries, id).map(|entry| &entry.port) {
        Some(EnginePort::Asr { build, .. }) => build(ctx),
        _ => Err(not_found(EngineKind::Asr, id)),
    }
}

fn build_polisher_in(
    entries: &[EngineEntry],
    id: &EngineId,
    ctx: &BuildCtx,
) -> PortResult<Arc<dyn TextPolisher>> {
    match entry_in(entries, id).map(|entry| &entry.port) {
        Some(EnginePort::Polisher { build, .. }) => build(ctx),
        _ => Err(not_found(EngineKind::Polisher, id)),
    }
}

fn build_vad_in(
    entries: &[EngineEntry],
    id: &EngineId,
    ctx: &BuildCtx,
) -> PortResult<Box<dyn VoiceActivity>> {
    match entry_in(entries, id).map(|entry| &entry.port) {
        Some(EnginePort::Vad { build, .. }) => build(ctx),
        _ => Err(not_found(EngineKind::Vad, id)),
    }
}

fn entry_in<'a>(entries: &'a [EngineEntry], id: &EngineId) -> Option<&'a EngineEntry> {
    entries.iter().find(|entry| entry.id == *id)
}

fn not_found(kind: EngineKind, id: &EngineId) -> PortError {
    PortError::new(AppError::NotFound {
        resource: ResourceKind::Engine,
    })
    .with_detail(format!("no {kind:?} engine is registered as `{id}`"))
}

#[cfg(test)]
pub(super) mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::{
        ports::fakes::{
            FakeAsrEngine, FakeModelStore, FakePolish, FakeTextPolisher, FakeVoiceActivity,
        },
        registry::tests::is_registry_id,
        types::{
            Accelerator, Language, LanguageSupport, LatencyClass, StaticList,
            testing::{TempDir, source_resource_paths},
        },
    };

    const LANGUAGES: &[Language] = &[Language::from_static("en"), Language::from_static("de")];

    pub(crate) const SAMPLE_ASR_CAPS: AsrCaps = AsrCaps {
        languages: StaticList::new(LANGUAGES),
        auto_language: true,
        punctuation: true,
        casing: true,
        accelerators: StaticList::new(&[Accelerator::Cpu, Accelerator::Gpu]),
        max_segment_s: 20,
    };

    const LLM_CAPS: PolisherCaps = PolisherCaps {
        latency_class: LatencyClass::Slow,
        languages: LanguageSupport::Any,
        needs_model: true,
    };

    const RULES_CAPS: PolisherCaps = PolisherCaps {
        latency_class: LatencyClass::Instant,
        languages: LanguageSupport::Any,
        needs_model: false,
    };

    const VAD_CAPS: VadCaps = VadCaps { frame_ms: 32 };

    fn build_fake_asr(_: &BuildCtx) -> PortResult<Arc<dyn AsrEngine>> {
        Ok(Arc::new(FakeAsrEngine::new(SAMPLE_ASR_CAPS)))
    }

    fn build_fake_llm(_: &BuildCtx) -> PortResult<Arc<dyn TextPolisher>> {
        Ok(Arc::new(FakeTextPolisher::new(
            LLM_CAPS,
            FakePolish::Map(str::to_owned),
        )))
    }

    fn build_fake_rules(_: &BuildCtx) -> PortResult<Arc<dyn TextPolisher>> {
        Ok(Arc::new(FakeTextPolisher::new(
            RULES_CAPS,
            FakePolish::Map(str::to_owned),
        )))
    }

    fn build_fake_vad(_: &BuildCtx) -> PortResult<Box<dyn VoiceActivity>> {
        Ok(Box::new(FakeVoiceActivity::new(VAD_CAPS.frame_ms)))
    }

    /// One entry per port, backed by the fakes: what the real entries will look like.
    pub(crate) const SAMPLE_ENGINES: &[EngineEntry] = &[
        EngineEntry {
            id: EngineId::from_static("sample-asr"),
            label: StaticStr::new("Sample ASR"),
            model_id: Some(ModelId::from_static("sample-asr-model")),
            port: EnginePort::Asr {
                caps: SAMPLE_ASR_CAPS,
                build: build_fake_asr,
            },
        },
        EngineEntry {
            id: EngineId::from_static("sample-llm"),
            label: StaticStr::new("Sample LLM"),
            model_id: Some(ModelId::from_static("sample-llm-model")),
            port: EnginePort::Polisher {
                caps: LLM_CAPS,
                build: build_fake_llm,
            },
        },
        EngineEntry {
            id: EngineId::from_static("sample-rules"),
            label: StaticStr::new("Sample rules"),
            model_id: None,
            port: EnginePort::Polisher {
                caps: RULES_CAPS,
                build: build_fake_rules,
            },
        },
        EngineEntry {
            id: EngineId::from_static("sample-vad"),
            label: StaticStr::new("Sample VAD"),
            model_id: None,
            port: EnginePort::Vad {
                caps: VAD_CAPS,
                build: build_fake_vad,
            },
        },
    ];

    fn ctx() -> BuildCtx {
        BuildCtx {
            paths: AppPaths::new("data", "resources"),
            model_store: Arc::new(FakeModelStore::new("data")),
        }
    }

    fn check_entries(entries: &[EngineEntry], known_model: impl Fn(&ModelId) -> bool) {
        let mut ids = HashSet::new();
        for entry in entries {
            assert!(is_registry_id(entry.id.as_str()), "{}", entry.id);
            assert!(
                ids.insert(entry.id.as_str()),
                "duplicate engine {}",
                entry.id
            );
            assert!(!entry.label.trim().is_empty(), "{} has no label", entry.id);
            assert_eq!(entry.spec().caps.kind(), entry.kind());
            if let Some(model_id) = &entry.model_id {
                assert!(
                    known_model(model_id),
                    "{} runs unknown model {model_id}",
                    entry.id
                );
            }
        }
    }

    #[test]
    fn every_engine_is_unique_and_runs_a_registered_model() {
        check_entries(ENGINES, |id| models::find(id).is_some());
        let sample_models = ["sample-asr-model", "sample-llm-model"];
        check_entries(SAMPLE_ENGINES, |id| sample_models.contains(&id.as_str()));
    }

    #[test]
    fn builders_construct_only_the_requested_kind() {
        let ctx = ctx();
        let asr = build_asr_in(SAMPLE_ENGINES, &EngineId::from_static("sample-asr"), &ctx);
        assert_eq!(asr.map(|engine| engine.caps()).ok(), Some(SAMPLE_ASR_CAPS));

        let llm = build_polisher_in(SAMPLE_ENGINES, &EngineId::from_static("sample-llm"), &ctx);
        assert_eq!(llm.map(|polisher| polisher.caps()).ok(), Some(LLM_CAPS));

        let vad = build_vad_in(SAMPLE_ENGINES, &EngineId::from_static("sample-vad"), &ctx);
        assert_eq!(vad.map(|detector| detector.caps()).ok(), Some(VAD_CAPS));

        let wrong_kind =
            build_polisher_in(SAMPLE_ENGINES, &EngineId::from_static("sample-asr"), &ctx);
        assert_eq!(
            wrong_kind.err().map(PortError::into_app_error),
            Some(AppError::NotFound {
                resource: ResourceKind::Engine
            })
        );
    }

    #[test]
    fn unknown_engines_fail_with_not_found_and_log_the_id() {
        let error = build_asr(&EngineId::from_static("missing"), &ctx())
            .err()
            .unwrap();
        assert_eq!(
            error.error(),
            &AppError::NotFound {
                resource: ResourceKind::Engine
            }
        );
        assert!(error.detail().unwrap().contains("missing"));
        assert!(build_vad(&EngineId::from_static("missing"), &ctx()).is_err());
        assert!(build_polisher(&EngineId::from_static("missing"), &ctx()).is_err());
    }

    #[test]
    fn the_default_vad_is_silero_on_the_bundled_model() {
        let data = TempDir::new("engines-vad");
        let ctx = BuildCtx {
            paths: source_resource_paths(data.path()),
            model_store: Arc::new(FakeModelStore::new(data.path())),
        };
        assert_eq!(default_vad().map(|entry| &entry.id), Some(&SILERO_VAD));
        let mut vad = build_default_vad(&ctx).unwrap();
        assert_eq!(vad.caps(), SileroVad::CAPS);
        vad.reset().unwrap();
        assert!(vad.push(&[0.0; 512]).is_ok());
    }

    #[test]
    fn specs_carry_caps_tagged_by_kind() {
        let spec = SAMPLE_ENGINES[0].spec();
        assert_eq!(spec.caps, EngineCaps::Asr(SAMPLE_ASR_CAPS));
        assert_eq!(specs().len(), ENGINES.len());
        assert_eq!(
            of_kind(EngineKind::Asr).count()
                + of_kind(EngineKind::Polisher).count()
                + of_kind(EngineKind::Vad).count(),
            ENGINES.len()
        );
        assert!(SAMPLE_ENGINES[2].manifest().is_none());
    }
}
