/*!
 * SOURCE OF TRUTH KEYWORDS: asr adapters, AsrEngine implementations, ParakeetOnnx, speech recognition engines
 * WHAT:  Adapters behind the AsrEngine port.
 * WHY:   A speech engine is an adapter plus a registry entry (02 §8.4): the registry builds the selected one
 *        (registry/engines.rs) and the ASR worker only sees `Arc<dyn AsrEngine>` and its caps. A new engine (Whisper,
 *        Moonshine) is a new folder here, never a change to the pipeline.
 * WHERE: Built by the `parakeet-tdt-0.6b-v3` registry entry.
 */

mod parakeet_onnx;

pub use parakeet_onnx::ParakeetOnnx;
