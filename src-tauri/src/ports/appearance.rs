/*!
 * SOURCE OF TRUTH KEYWORDS: SystemAppearance, transparency effects, EnableTransparency, Mica support, appearance listener, AppearanceCaps
 * WHAT:  SystemAppearance reports the operating system's appearance facts Echo cannot read from CSS: whether
 *        "Transparency effects" are on (and every later change, pushed to an EventSink), and whether the OS offers
 *        the Mica material (caps).
 * WHY:   A webview cannot see the Windows transparency switch or the OS build, yet the UI must switch its glass
 *        tints to solids when transparency is off (05 W17) and paint its own background when Mica is missing
 *        (04 §2). Both are Windows API calls, so they sit behind a port (root CLAUDE.md §3). `transparency` is a
 *        cheap blocking read; a failed read is reported so the caller can log it and pick the readable solids.
 *        `listen` replaces the previous sink, like the other event ports.
 * WHERE: Implemented by adapters/appearance (Win32SystemAppearance) and ports/fakes; read by pipeline/appearance
 *        (view + relay), ipc/commands/system.rs and app/windows.rs (whether to apply Mica).
 */

use std::sync::Arc;

use super::EventSink;
use crate::types::{AppearanceCaps, PortResult, Transparency};

/// Operating-system appearance preferences.
pub trait SystemAppearance: Send + Sync {
    fn caps(&self) -> AppearanceCaps;

    /// Whether Windows "Transparency effects" are on right now.
    fn transparency(&self) -> PortResult<Transparency>;

    /// Sends every later change of the transparency switch to `sink`, replacing the previous sink.
    fn listen(&self, sink: Arc<dyn EventSink<Transparency>>) -> PortResult<()>;
}
