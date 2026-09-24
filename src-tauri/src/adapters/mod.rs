/*!
 * SOURCE OF TRUTH KEYWORDS: adapters layer, port implementations, Windows integrations, local AI engines, ONNX, Win32
 * WHAT:  Layer 2: concrete implementations of ports (local ASR/VAD/LLM, WASAPI, Win32, clipboard, HTTP).
 * WHY:   All Windows API calls and model-specific code are confined here (02 §3.5, §9). Only the adapter the
 *        registry selects is constructed; callers branch on its declared caps, never on its name.
 * WHERE: Built by registry/ entries and app/; may import types/ and ports/ only.
 */

pub mod appearance;
pub mod asr;
pub mod audio;
pub mod clipboard;
pub mod consent;
pub mod foreground;
pub mod hotkey;
pub mod inserter;
pub mod launcher;
pub mod notifier;
pub mod onnx;
pub mod polish;
pub mod scheduler;
pub mod vad;
pub mod win32;
pub mod window;
