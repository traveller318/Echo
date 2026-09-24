/*!
 * SOURCE OF TRUTH KEYWORDS: setting enum values, AUTO, TOGGLE, HOLD, CPU, GPU, stored enum spelling
 * WHAT:  Enum values the core compares against (the rest are only shown and stored).
 * WHY:   Only the registry spells stored enum values, so the pipeline never compares string literals.
 * WHERE: registry/settings (list, reads, options); pipeline tests.
 */

/// `transcription.language` / `transcription.accelerator`: let the engine decide.
pub const AUTO: &str = "auto";
/// `hotkeys.mode`: press to start, press again to stop.
pub const TOGGLE: &str = "toggle";
/// `hotkeys.mode`: record while the keys are held.
pub const HOLD: &str = "hold";
/// `transcription.accelerator`: run on the processor.
pub const CPU: &str = "cpu";
/// `transcription.accelerator`: run on the graphics card (DirectML).
pub const GPU: &str = "gpu";
