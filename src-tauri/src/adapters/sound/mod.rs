/*!
 * SOURCE OF TRUTH KEYWORDS: sound adapters, SoundPlayer implementations, Win32SoundPlayer, PlaySound, sound cues output
 * WHAT:  Adapters behind the SoundPlayer port.
 * WHY:   Audio output is an OS integration and stays behind its port (root CLAUDE.md §3); a richer backend (a WASAPI
 *        render stream with its own volume, user-picked sounds) is a new file here, never a pipeline change.
 * WHERE: Constructed by app/bootstrap; used only through `dyn SoundPlayer` by pipeline/sound_cues.rs.
 */

mod win32_play_sound;

pub use win32_play_sound::Win32SoundPlayer;
