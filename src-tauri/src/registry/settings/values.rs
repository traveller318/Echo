/*!
 * SOURCE OF TRUTH KEYWORDS: setting enum values, AUTO, TOGGLE, HOLD, CPU, GPU, stored enum spelling, pill position spelling, parse_pill_position
 * WHAT:  Enum values the core compares against (the rest are only shown and stored), and the stored spelling of
 *        the one structured hidden value, the pill's dragged position (`pill_position` / `parse_pill_position`).
 * WHY:   Only the registry spells stored values, so the pipeline never compares string literals or parses text.
 *        The position is kept as `"x,y"` text (physical pixels of the window's top-left corner) because it is one
 *        value that is either set or not: two Int settings could be half written, and "not dragged yet" would
 *        need a sentinel number.
 * WHERE: registry/settings (list, reads, options); pipeline tests; ipc/commands/pill.rs (storing a drag).
 */

use crate::types::{ScreenPoint, SettingValue, StaticStr};

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

/// Longest `pill.position` text: two i32 values and a comma.
pub const PILL_POSITION_MAX_LEN: u32 = 23;

/// The stored `pill.position` value for a window corner at `point`.
pub fn pill_position(point: ScreenPoint) -> SettingValue {
    SettingValue::Text(StaticStr::from(format!("{},{}", point.x, point.y)))
}

/// The corner a stored `pill.position` text names; None when it is empty (never dragged) or not a position.
pub fn parse_pill_position(text: &str) -> Option<ScreenPoint> {
    let (x, y) = text.split_once(',')?;
    Some(ScreenPoint {
        x: x.trim().parse().ok()?,
        y: y.trim().parse().ok()?,
    })
}
