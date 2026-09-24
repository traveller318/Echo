/*!
 * SOURCE OF TRUTH KEYWORDS: vad adapters, VoiceActivity implementations, SileroVad, voice activity detection
 * WHAT:  Adapters behind the VoiceActivity port.
 * WHY:   A detector is an engine: the registry builds the selected one (registry/engines.rs) and the capture worker
 *        only sees `Box<dyn VoiceActivity>` and its caps (02 §3.5).
 * WHERE: Built by the `silero-vad-v5` registry entry.
 */

mod silero;

pub use silero::SileroVad;
