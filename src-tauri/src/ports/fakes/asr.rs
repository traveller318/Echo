/*!
 * SOURCE OF TRUTH KEYWORDS: FakeAsrEngine, AsrCall, fake speech recognition, scripted transcripts, load fallback, warm up count
 * WHAT:  FakeAsrEngine: an AsrEngine that returns scripted results in order and records every call (AsrCall).
 * WHY:   Pipeline tests need exact segment texts (to test ordered joins), failures (to test `failed` takes and
 *        retry) and the GPU → CPU fallback (05 A6) without a model. It enforces the port contract: the
 *        accelerator must be declared in caps, the engine must be loaded, a segment may not exceed
 *        `max_segment_s`, and auto-detect needs `auto_language`. An unscripted call fails loudly instead of
 *        inventing text.
 * WHERE: pipeline ASR worker, model switching and session actor tests.
 */

use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::Mutex,
};

use super::lock;
use crate::{
    ports::AsrEngine,
    types::{
        Accelerator, AppError, AsrCaps, AsrOutput, Language, PIPELINE_SAMPLE_RATE_HZ, PortError,
        PortResult, StaticList,
    },
};

/// One `transcribe` call as the fake saw it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsrCall {
    pub samples: usize,
    pub language: Option<Language>,
}

#[derive(Default)]
struct AsrState {
    loaded: Option<(PathBuf, Accelerator)>,
    gpu_falls_back: bool,
    next_load_error: Option<PortError>,
    warm_ups: usize,
    script: VecDeque<PortResult<AsrOutput>>,
    calls: Vec<AsrCall>,
}

/// A scripted speech recognition engine.
pub struct FakeAsrEngine {
    caps: AsrCaps,
    state: Mutex<AsrState>,
}

impl FakeAsrEngine {
    pub fn new(caps: AsrCaps) -> Self {
        Self {
            caps,
            state: Mutex::default(),
        }
    }

    /// English only, punctuated and cased output, CPU and GPU, 20 s segments.
    pub fn english() -> Self {
        Self::new(AsrCaps {
            languages: StaticList::from(vec![Language::from_static("en")]),
            auto_language: false,
            punctuation: true,
            casing: true,
            accelerators: StaticList::from(vec![Accelerator::Cpu, Accelerator::Gpu]),
            max_segment_s: 20,
        })
    }

    /// Queues the text the next `transcribe` returns.
    pub fn push_text(&self, text: &str) {
        self.push_result(Ok(AsrOutput {
            text: text.to_owned(),
            language: None,
        }));
    }

    /// Queues the result the next `transcribe` returns.
    pub fn push_result(&self, result: PortResult<AsrOutput>) {
        lock(&self.state).script.push_back(result);
    }

    /// A GPU load succeeds on the CPU instead, as when a DirectML session cannot start.
    pub fn fall_back_to_cpu(&self) {
        lock(&self.state).gpu_falls_back = true;
    }

    pub fn fail_next_load(&self, error: PortError) {
        lock(&self.state).next_load_error = Some(error);
    }

    /// The loaded model folder and accelerator in use.
    pub fn loaded(&self) -> Option<(PathBuf, Accelerator)> {
        lock(&self.state).loaded.clone()
    }

    pub fn warm_ups(&self) -> usize {
        lock(&self.state).warm_ups
    }

    pub fn calls(&self) -> Vec<AsrCall> {
        lock(&self.state).calls.clone()
    }

    fn not_loaded() -> PortError {
        PortError::new(AppError::Asr).with_detail("fake ASR: no model loaded")
    }
}

impl AsrEngine for FakeAsrEngine {
    fn caps(&self) -> AsrCaps {
        self.caps.clone()
    }

    fn load(&self, model_dir: &Path, accelerator: Accelerator) -> PortResult<Accelerator> {
        if !self.caps.supports_accelerator(accelerator) {
            return Err(PortError::new(AppError::Internal)
                .with_detail("fake ASR: accelerator not declared in caps"));
        }
        let mut state = lock(&self.state);
        if let Some(error) = state.next_load_error.take() {
            return Err(error);
        }
        let in_use = match accelerator {
            Accelerator::Gpu if state.gpu_falls_back => Accelerator::Cpu,
            requested => requested,
        };
        state.loaded = Some((model_dir.to_path_buf(), in_use));
        Ok(in_use)
    }

    fn warm_up(&self) -> PortResult<()> {
        let mut state = lock(&self.state);
        if state.loaded.is_none() {
            return Err(Self::not_loaded());
        }
        state.warm_ups += 1;
        Ok(())
    }

    fn unload(&self) -> PortResult<()> {
        lock(&self.state).loaded = None;
        Ok(())
    }

    fn transcribe(&self, audio: &[f32], language: Option<&Language>) -> PortResult<AsrOutput> {
        let mut state = lock(&self.state);
        if state.loaded.is_none() {
            return Err(Self::not_loaded());
        }
        let max_samples = usize::try_from(
            u64::from(self.caps.max_segment_s) * u64::from(PIPELINE_SAMPLE_RATE_HZ),
        )
        .unwrap_or(usize::MAX);
        let language_ok = match language {
            Some(language) => self.caps.languages.contains(language),
            None => self.caps.auto_language,
        };
        if audio.len() > max_samples || !language_ok {
            return Err(PortError::new(AppError::Internal)
                .with_detail("fake ASR: segment too long or language not in caps"));
        }
        state.calls.push(AsrCall {
            samples: audio.len(),
            language: language.cloned(),
        });
        state.script.pop_front().unwrap_or_else(|| {
            Err(PortError::new(AppError::Asr).with_detail("fake ASR: no scripted result"))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EN: Language = Language::from_static("en");

    #[test]
    fn transcribes_scripted_text_in_order_once_loaded() {
        let engine = FakeAsrEngine::english();
        assert!(engine.transcribe(&[0.0; 16], Some(&EN)).is_err());
        assert_eq!(
            engine
                .load(Path::new("models/fake"), Accelerator::Cpu)
                .unwrap(),
            Accelerator::Cpu
        );
        engine.warm_up().unwrap();
        engine.push_text("Hello.");
        engine.push_text("World.");
        assert_eq!(
            engine.transcribe(&[0.0; 16], Some(&EN)).unwrap().text,
            "Hello."
        );
        assert_eq!(
            engine.transcribe(&[0.0; 32], Some(&EN)).unwrap().text,
            "World."
        );
        assert_eq!(
            engine.calls(),
            [
                AsrCall {
                    samples: 16,
                    language: Some(EN)
                },
                AsrCall {
                    samples: 32,
                    language: Some(EN)
                },
            ]
        );
        assert!(engine.transcribe(&[0.0; 16], Some(&EN)).is_err());
        assert_eq!(engine.warm_ups(), 1);
    }

    #[test]
    fn enforces_caps() {
        let engine = FakeAsrEngine::english();
        engine
            .load(Path::new("models/fake"), Accelerator::Cpu)
            .unwrap();
        engine.push_text("unused");
        assert!(engine.transcribe(&[0.0; 16], None).is_err());
        assert!(
            engine
                .transcribe(&[0.0; 16], Some(&Language::from_static("de")))
                .is_err()
        );
        assert!(
            engine
                .transcribe(&vec![0.0; 20 * 16_000 + 1], Some(&EN))
                .is_err()
        );
        assert!(engine.calls().is_empty());
    }

    #[test]
    fn gpu_can_fall_back_to_cpu_and_unload_forgets_the_model() {
        let engine = FakeAsrEngine::english();
        engine.fall_back_to_cpu();
        assert_eq!(
            engine
                .load(Path::new("models/fake"), Accelerator::Gpu)
                .unwrap(),
            Accelerator::Cpu
        );
        engine.unload().unwrap();
        assert_eq!(engine.loaded(), None);
        assert!(engine.warm_up().is_err());
    }
}
