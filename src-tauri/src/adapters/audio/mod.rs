/*!
 * SOURCE OF TRUTH KEYWORDS: audio adapters, AudioCapture implementations, CpalWasapiCapture, microphone, WASAPI, device watch, endpoint bus
 * WHAT:  Adapters behind the AudioCapture port, plus the Core Audio pieces the WASAPI one uses: endpoint
 *        properties (endpoints.rs) and the hot-plug watch (device_watch.rs).
 * WHY:   Microphone access is a Windows API concern and stays behind its port (root CLAUDE.md §3); a second backend
 *        (WASAPI exclusive, a file source) is a new file here, never a pipeline change.
 * WHERE: Constructed by app/bootstrap; used only through `dyn AudioCapture`.
 */

mod cpal_wasapi;
mod device_watch;
mod endpoints;

pub use cpal_wasapi::CpalWasapiCapture;
