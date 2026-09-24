/*!
 * SOURCE OF TRUTH KEYWORDS: AsrOutput, transcription result, segment text, detected language, ASR engine output
 * WHAT:  AsrOutput: what an ASR engine returns for one audio segment (text plus the language it detected).
 * WHY:   Every engine (Parakeet today, Whisper or Moonshine later, 02 §8.4) returns this one shape, so the ASR
 *        worker and segment join never know which engine ran. The detected language is optional because only
 *        engines with `AsrCaps.auto_language` report one; it is stored as `transcripts.language`.
 * WHERE: Returned by `AsrEngine::transcribe` (ports/asr.rs); consumed by pipeline/asr_worker.rs, which joins
 *        segments by index and hands the text to the polish chain.
 */

use super::Language;

/// Text an ASR engine produced for one segment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsrOutput {
    /// Recognized text, trimmed; empty when the segment held no words.
    pub text: String,
    /// The language the engine detected, when it reports one.
    pub language: Option<Language>,
}
