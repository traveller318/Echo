/*!
 * SOURCE OF TRUTH KEYWORDS: LinuxSystemAppearance, no Mica on Linux, transparency Linux
 * WHAT:  LinuxSystemAppearance: SystemAppearance with no Mica backdrop and full transparency that never changes.
 * WHY:   Mica is a Windows 11 material; on Linux the page paints its own background (the CSS fallback the caps
 *        select, 04 §2) and the light/dark theme reaches WebKitGTK through `prefers-color-scheme` from the GTK theme.
 *        No desktop exposes a "transparency effects" switch apps are expected to follow, so there is nothing to
 *        watch and `listen` only keeps the contract.
 * WHERE: Built by app/bootstrap on Linux; read by app/windows.rs and pipeline/appearance.rs.
 */

use std::sync::Arc;

use crate::{
    ports::{EventSink, SystemAppearance},
    types::{AppearanceCaps, PortResult, Transparency},
};

#[derive(Debug, Default)]
pub struct LinuxSystemAppearance;

impl LinuxSystemAppearance {
    pub const fn new() -> Self {
        Self
    }
}

impl SystemAppearance for LinuxSystemAppearance {
    fn caps(&self) -> AppearanceCaps {
        AppearanceCaps { mica: false }
    }

    fn transparency(&self) -> PortResult<Transparency> {
        Ok(Transparency::Full)
    }

    fn listen(&self, _sink: Arc<dyn EventSink<Transparency>>) -> PortResult<()> {
        Ok(())
    }
}
