/*!
 * SOURCE OF TRUTH KEYWORDS: NoGraphicsAdapters, Linux GPU list, CPU-only ONNX Runtime
 * WHAT:  NoGraphicsAdapters: GraphicsAdapters that lists no GPU.
 * WHY:   The accelerators Echo ships are DirectML (Windows only); the Linux ONNX Runtime bundle is the CPU build, so
 *        an empty list makes the accelerator picker choose the CPU and the UI hide GPU options (02 §8.1).
 * WHERE: Built by app/bootstrap on Linux into the AcceleratorPicker.
 */

use crate::{
    ports::GraphicsAdapters,
    types::{GpuAdapter, PortResult},
};

#[derive(Debug, Default)]
pub struct NoGraphicsAdapters;

impl NoGraphicsAdapters {
    pub const fn new() -> Self {
        Self
    }
}

impl GraphicsAdapters for NoGraphicsAdapters {
    fn list(&self) -> PortResult<Vec<GpuAdapter>> {
        Ok(Vec::new())
    }
}
