/*!
 * SOURCE OF TRUTH KEYWORDS: audio adapters, AudioCapture implementations, CpalWasapiCapture, microphone, WASAPI
 * WHAT:  Adapters behind the AudioCapture port.
 * WHY:   Microphone access is a Windows API concern and stays behind its port (root CLAUDE.md §3); a second backend
 *        (WASAPI exclusive, a file source) is a new file here, never a pipeline change.
 * WHERE: Constructed by app/bootstrap; used only through `dyn AudioCapture`.
 */

mod cpal_wasapi;

pub use cpal_wasapi::CpalWasapiCapture;
