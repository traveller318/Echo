/*!
 * SOURCE OF TRUTH KEYWORDS: ParakeetOnnx, Parakeet TDT 0.6B v3, AsrEngine adapter, Parakeet caps, 25 languages, model files, ModelMissing, warm up
 * WHAT:  ParakeetOnnx: the AsrEngine for NVIDIA Parakeet TDT 0.6B v3 (int8 ONNX export) on the bundled ONNX Runtime.
 *        `load` checks the four model files, opens the sessions (model.rs) on the CPU with 05 A9 threads; `warm_up`
 *        transcribes 1 s of silence (05 A8); `transcribe` runs one segment; `unload` frees the sessions.
 * WHY:   The only code that knows Parakeet's files, tensors and decoding (00 constraint 4): the rest of Echo sees
 *        `Arc<dyn AsrEngine>` and CAPS. The caps are honest: the 25 European languages the model card lists, detected
 *        by the model itself (it takes no language hint, so the requested language is not passed on and no detected
 *        language is reported), punctuation and casing in the output, CPU only (the DirectML session is not built
 *        here, so GPU is not declared), and 30 s per call (the pipeline cuts at 20 s; full attention over 30 s fits
 *        comfortably in memory). A missing file is `ModelMissing` before any session opens (05 A1), so onboarding
 *        can offer the download; the loaded model sits behind a mutex because ONNX sessions run through `&mut` while
 *        the port shares the engine as `&self` (the ASR worker is its only caller, so the lock never contends).
 *        Loading replaces a loaded model after dropping it, so two copies (about 1 GB each, 05 A7) never coexist
 *        inside one engine.
 * WHERE: Built by the `parakeet-tdt-0.6b-v3` registry engine entry (registry/engines.rs); driven by the ASR worker
 *        (pipeline/asr). FILES is checked against the registry manifest by the registry tests.
 */

mod model;
mod tdt;
mod vocab;

use std::path::Path;

use parking_lot::Mutex;

use self::model::ParakeetModel;
use crate::{
    adapters::onnx::SessionThreads,
    ports::AsrEngine,
    types::{
        Accelerator, AppError, AppPaths, AsrCaps, AsrOutput, Language, ModelId,
        PIPELINE_SAMPLE_RATE_HZ, PortError, PortResult, StaticList,
    },
};

/// The int8 encoder (the bulk of the model).
const ENCODER: &str = "encoder-model.int8.onnx";
/// The int8 prediction + joint network.
const JOINT: &str = "decoder_joint-model.int8.onnx";
/// The 128-bin log-mel preprocessor.
const PREPROCESSOR: &str = "nemo128.onnx";
/// The SentencePiece token table.
const VOCAB: &str = "vocab.txt";

/// Samples of silence the warm-up transcribes (1 s, 05 A8).
const WARM_UP_SAMPLES: usize = PIPELINE_SAMPLE_RATE_HZ as usize;

/// The languages of the Parakeet TDT 0.6B v3 model card, in its order.
const LANGUAGES: &[Language] = &[
    Language::from_static("en"),
    Language::from_static("es"),
    Language::from_static("fr"),
    Language::from_static("de"),
    Language::from_static("bg"),
    Language::from_static("hr"),
    Language::from_static("cs"),
    Language::from_static("da"),
    Language::from_static("nl"),
    Language::from_static("et"),
    Language::from_static("fi"),
    Language::from_static("el"),
    Language::from_static("hu"),
    Language::from_static("it"),
    Language::from_static("lv"),
    Language::from_static("lt"),
    Language::from_static("mt"),
    Language::from_static("pl"),
    Language::from_static("pt"),
    Language::from_static("ro"),
    Language::from_static("sk"),
    Language::from_static("sl"),
    Language::from_static("sv"),
    Language::from_static("ru"),
    Language::from_static("uk"),
];

/// Parakeet TDT on ONNX Runtime.
pub struct ParakeetOnnx {
    paths: AppPaths,
    model_id: ModelId,
    model: Mutex<Option<ParakeetModel>>,
}

impl ParakeetOnnx {
    /// What this engine declares.
    pub const CAPS: AsrCaps = AsrCaps {
        languages: StaticList::new(LANGUAGES),
        auto_language: true,
        punctuation: true,
        casing: true,
        accelerators: StaticList::new(&[Accelerator::Cpu]),
        max_segment_s: 30,
    };

    /// Every file `load` needs in the model folder; the registry manifest lists exactly these.
    pub const FILES: [&str; 4] = [ENCODER, JOINT, PREPROCESSOR, VOCAB];

    /// An unloaded engine; `paths` locates the bundled ONNX Runtime, `model_id` names the model in errors.
    pub fn new(paths: AppPaths, model_id: ModelId) -> Self {
        Self {
            paths,
            model_id,
            model: Mutex::new(None),
        }
    }

    fn not_loaded() -> PortError {
        PortError::new(AppError::Asr).with_detail("Parakeet: no model is loaded")
    }
}

impl AsrEngine for ParakeetOnnx {
    fn caps(&self) -> AsrCaps {
        Self::CAPS
    }

    fn load(&self, model_dir: &Path, accelerator: Accelerator) -> PortResult<Accelerator> {
        if !Self::CAPS.supports_accelerator(accelerator) {
            return Err(PortError::new(AppError::Internal).with_detail(format!(
                "Parakeet does not declare the {accelerator:?} accelerator"
            )));
        }
        let missing: Vec<&str> = Self::FILES
            .into_iter()
            .filter(|file| !model_dir.join(file).is_file())
            .collect();
        if !missing.is_empty() {
            return Err(PortError::new(AppError::ModelMissing {
                model_id: self.model_id.clone(),
            })
            .with_detail(format!("{} lacks {missing:?}", model_dir.display())));
        }
        let mut model = self.model.lock();
        // Free the old sessions first, so a reload never holds two copies of the model.
        *model = None;
        *model = Some(ParakeetModel::open(
            &self.paths,
            model_dir,
            &self.model_id,
            SessionThreads::for_asr(),
        )?);
        Ok(Accelerator::Cpu)
    }

    fn warm_up(&self) -> PortResult<()> {
        let mut model = self.model.lock();
        let model = model.as_mut().ok_or_else(Self::not_loaded)?;
        model.transcribe(&[0.0; WARM_UP_SAMPLES]).map(drop)
    }

    fn unload(&self) -> PortResult<()> {
        *self.model.lock() = None;
        Ok(())
    }

    fn transcribe(&self, audio: &[f32], _language: Option<&Language>) -> PortResult<AsrOutput> {
        let limit = Self::CAPS.max_segment_s as usize * PIPELINE_SAMPLE_RATE_HZ as usize;
        if audio.len() > limit {
            return Err(PortError::new(AppError::Internal).with_detail(format!(
                "Parakeet: a segment of {} samples exceeds the declared {} s",
                audio.len(),
                Self::CAPS.max_segment_s
            )));
        }
        let mut model = self.model.lock();
        let model = model.as_mut().ok_or_else(Self::not_loaded)?;
        Ok(AsrOutput {
            text: model.transcribe(audio)?,
            language: None,
        })
    }
}

/**
 * SOURCE OF TRUTH KEYWORDS: Parakeet real model test, installed model, speech fixture transcription
 * WHAT:  Runs the real model from the developer's install (`models/parakeet-tdt-0.6b-v3/` in the Echo data folder)
 *        on the speech fixture, plus the checks that need no model (missing files, contract errors).
 * WHY:   Decoding, detokenizing and the export's signature can only be proven on the real files; a missing install
 *        fails with the folder to fill instead of passing silently.
 * WHERE: `cargo test`.
 */
#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::types::testing::{TempDir, installed_app_paths, source_resource_paths};

    const MODEL: ModelId = ModelId::from_static("parakeet-tdt-0.6b-v3");

    /// A loaded engine on the installed model.
    fn installed() -> ParakeetOnnx {
        let paths = installed_app_paths();
        let dir = paths.model_dir(&MODEL);
        for file in ParakeetOnnx::FILES {
            assert!(
                dir.join(file).is_file(),
                "the Parakeet model is not installed: put the four files of registry/models.rs into {}",
                dir.display()
            );
        }
        let engine = ParakeetOnnx::new(paths, MODEL);
        assert_eq!(
            engine.load(&dir, Accelerator::Cpu).unwrap(),
            Accelerator::Cpu
        );
        engine
    }

    fn fixture() -> Vec<f32> {
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/speech-en-16k.wav");
        hound::WavReader::open(path)
            .unwrap()
            .into_samples::<i16>()
            .map(|sample| f32::from(sample.unwrap()) / f32::from(i16::MAX))
            .collect()
    }

    #[test]
    fn transcribes_recorded_speech_with_punctuation_and_casing() {
        let engine = installed();
        engine.warm_up().unwrap();
        let output = engine.transcribe(&fixture(), None).unwrap();
        assert_eq!(
            output.text,
            "Hello there. This is a short test of voice activity detection."
        );
        assert_eq!(output.language, None, "Parakeet does not report a language");
        let again = engine.transcribe(&fixture(), Some(&LANGUAGES[0])).unwrap();
        assert_eq!(
            again.text, output.text,
            "decoding is deterministic and ignores the hint"
        );
    }

    #[test]
    fn silence_is_empty_text() {
        let engine = installed();
        let output = engine
            .transcribe(&[0.0; 32_000], Some(&LANGUAGES[0]))
            .unwrap();
        assert_eq!(output.text, "");
        assert_eq!(engine.transcribe(&[], None).unwrap().text, "");
    }

    #[test]
    fn missing_files_are_model_missing_before_anything_opens() {
        let data = TempDir::new("parakeet-missing");
        let engine = ParakeetOnnx::new(source_resource_paths(data.path()), MODEL);
        std::fs::write(
            data.join(VOCAB),
            "<blk> 0
",
        )
        .unwrap();
        let error = engine.load(data.path(), Accelerator::Cpu).err().unwrap();
        assert_eq!(error.error(), &AppError::ModelMissing { model_id: MODEL });
        assert!(error.detail().unwrap().contains(ENCODER));
        assert!(!error.detail().unwrap().contains(VOCAB));
    }

    #[test]
    fn damaged_files_are_model_corrupt() {
        let data = TempDir::new("parakeet-corrupt");
        for file in ParakeetOnnx::FILES {
            std::fs::write(data.join(file), b"not a model").unwrap();
        }
        let engine = ParakeetOnnx::new(source_resource_paths(data.path()), MODEL);
        assert_eq!(
            engine
                .load(data.path(), Accelerator::Cpu)
                .err()
                .map(PortError::into_app_error),
            Some(AppError::ModelCorrupt { model_id: MODEL })
        );
        assert!(
            engine.warm_up().is_err(),
            "a failed load leaves nothing loaded"
        );
    }

    #[test]
    fn contract_violations_are_refused() {
        let engine = ParakeetOnnx::new(AppPaths::new("data", "resources"), MODEL);
        assert_eq!(
            engine
                .load(Path::new("models"), Accelerator::Gpu)
                .err()
                .map(PortError::into_app_error),
            Some(AppError::Internal),
            "GPU is not declared"
        );
        assert_eq!(
            engine
                .transcribe(&[0.0; 16], None)
                .err()
                .map(PortError::into_app_error),
            Some(AppError::Asr),
            "nothing is loaded"
        );
        let too_long = vec![0.0; 30 * 16_000 + 1];
        assert_eq!(
            engine
                .transcribe(&too_long, None)
                .err()
                .map(PortError::into_app_error),
            Some(AppError::Internal)
        );
        engine.unload().unwrap();
        assert_eq!(ParakeetOnnx::CAPS.languages.len(), 25);
    }
}
