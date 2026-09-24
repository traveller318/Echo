/*!
 * SOURCE OF TRUTH KEYWORDS: load_request, AsrLoadRequest from settings, effective_language, effective_accelerator, engine model folder, caps narrowing
 * WHAT:  `load_request` turns the current settings into the AsrLoadRequest for the selected engine (its registry id,
 *        the folder its model is installed in, the accelerator to ask for); `effective_accelerator` and
 *        `effective_language` narrow a stored preference to what an engine's caps offer.
 * WHY:   The worker loads whatever it is asked to, so the one place that reads settings, the registry and the data
 *        layout for it is here (00 constraint 4: nothing names a model). The model folder is `AppPaths::model_dir`,
 *        the folder the model manager installs into (02 §8.2); a missing model is the adapter's `ModelMissing` when
 *        it loads. `auto` (no preference) resolves to the CPU, which every engine supports; a preference the engine
 *        does not declare falls back the same way (05 A6), so an engine is never asked for an accelerator it lacks.
 *        A language the engine does not know falls back to auto-detect when it has one, otherwise to its first
 *        language, because `transcribe` accepts no language (None) only from engines with `auto_language`.
 * WHERE: app/bootstrap (startup load) and the engine switch call `load_request`; the ASR worker applies
 *        `effective_language` per segment with the caps of the engine the take is pinned to.
 */

use crate::{
    registry,
    types::{
        Accelerator, AppError, AppPaths, AsrCaps, AsrLoadRequest, EngineKind, Language, PortError,
        PortResult, ResourceKind, SettingsSnapshot,
    },
};

/// The load request for the ASR engine `transcription.engine` selects.
pub fn load_request(settings: &SettingsSnapshot, paths: &AppPaths) -> PortResult<AsrLoadRequest> {
    let engine_id = registry::settings::asr_engine(settings).ok_or_else(|| {
        PortError::new(AppError::NotFound {
            resource: ResourceKind::Engine,
        })
        .with_detail("transcription.engine holds no engine id")
    })?;
    let entry = registry::engines::find(&engine_id)
        .filter(|entry| entry.kind() == EngineKind::Asr)
        .ok_or_else(|| {
            PortError::new(AppError::NotFound {
                resource: ResourceKind::Engine,
            })
            .with_detail(format!("no ASR engine is registered as `{engine_id}`"))
        })?;
    let caps = entry.asr_caps().ok_or_else(|| {
        PortError::new(AppError::Internal)
            .with_detail(format!("ASR engine `{engine_id}` declares no ASR caps"))
    })?;
    let manifest = entry.manifest().ok_or_else(|| {
        PortError::new(AppError::Internal).with_detail(format!(
            "ASR engine `{engine_id}` names no registered model"
        ))
    })?;
    Ok(AsrLoadRequest {
        model_dir: paths.model_dir(&manifest.id),
        accelerator: effective_accelerator(
            registry::settings::accelerator_preference(settings),
            caps,
        ),
        engine_id,
    })
}

/// The accelerator to request: the preference when the engine declares it, otherwise the CPU (or, for an engine
/// without a CPU path, the first it declares).
pub fn effective_accelerator(preference: Option<Accelerator>, caps: &AsrCaps) -> Accelerator {
    match preference {
        Some(accelerator) if caps.supports_accelerator(accelerator) => accelerator,
        _ if caps.supports_accelerator(Accelerator::Cpu) => Accelerator::Cpu,
        _ => caps
            .accelerators
            .first()
            .copied()
            .unwrap_or(Accelerator::Cpu),
    }
}

/// The language to pass to `transcribe`: the preference when the engine knows it, otherwise auto-detect (None)
/// when the engine has it, otherwise the engine's first language.
pub fn effective_language(preference: Option<&Language>, caps: &AsrCaps) -> Option<Language> {
    match preference {
        Some(language) if caps.languages.contains(language) => Some(language.clone()),
        _ if caps.auto_language => None,
        _ => caps.languages.first().cloned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        registry::{
            engines::PARAKEET_TDT_V3,
            settings::{defaults, keys},
        },
        types::{SettingValue, StaticList, StaticStr},
    };

    const EN: Language = Language::from_static("en");
    const DE: Language = Language::from_static("de");
    const JA: Language = Language::from_static("ja");

    fn caps(auto_language: bool, accelerators: Vec<Accelerator>) -> AsrCaps {
        AsrCaps {
            languages: StaticList::from(vec![EN, DE]),
            auto_language,
            punctuation: true,
            casing: true,
            accelerators: StaticList::from(accelerators),
            max_segment_s: 20,
        }
    }

    #[test]
    fn the_default_settings_load_parakeet_from_its_model_folder_on_the_cpu() {
        let paths = AppPaths::new("data", "resources");
        let request = load_request(&defaults(), &paths).unwrap();
        assert_eq!(request.engine_id, PARAKEET_TDT_V3);
        assert_eq!(
            request.model_dir,
            paths.model_dir(&registry::models::PARAKEET_TDT_V3)
        );
        assert_eq!(request.accelerator, Accelerator::Cpu);
    }

    #[test]
    fn a_missing_or_non_asr_engine_is_not_found() {
        // Built directly: `resolve` would drop the invalid value and fall back to the default engine.
        let vad = SettingsSnapshot::from_resolved([(
            keys::ASR_ENGINE,
            SettingValue::Enum(StaticStr::new("silero-vad-v5")),
        )]);
        let paths = AppPaths::new("data", "resources");
        for settings in [vad, SettingsSnapshot::default()] {
            assert_eq!(
                load_request(&settings, &paths)
                    .err()
                    .map(PortError::into_app_error),
                Some(AppError::NotFound {
                    resource: ResourceKind::Engine
                })
            );
        }
    }

    #[test]
    fn accelerator_preferences_narrow_to_the_caps() {
        let both = caps(true, vec![Accelerator::Cpu, Accelerator::Gpu]);
        let cpu_only = caps(true, vec![Accelerator::Cpu]);
        let gpu_only = caps(true, vec![Accelerator::Gpu]);
        assert_eq!(
            effective_accelerator(Some(Accelerator::Gpu), &both),
            Accelerator::Gpu
        );
        assert_eq!(effective_accelerator(None, &both), Accelerator::Cpu);
        assert_eq!(
            effective_accelerator(Some(Accelerator::Gpu), &cpu_only),
            Accelerator::Cpu
        );
        assert_eq!(
            effective_accelerator(Some(Accelerator::Cpu), &gpu_only),
            Accelerator::Gpu
        );
    }

    #[test]
    fn language_preferences_narrow_to_the_caps() {
        let auto = caps(true, vec![Accelerator::Cpu]);
        let fixed = caps(false, vec![Accelerator::Cpu]);
        assert_eq!(effective_language(Some(&DE), &auto), Some(DE));
        assert_eq!(effective_language(None, &auto), None);
        assert_eq!(
            effective_language(Some(&JA), &auto),
            None,
            "unknown → auto-detect"
        );
        assert_eq!(
            effective_language(None, &fixed),
            Some(EN),
            "no auto-detect → first language"
        );
        assert_eq!(effective_language(Some(&JA), &fixed), Some(EN));
    }
}
