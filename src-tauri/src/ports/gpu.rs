/*!
 * SOURCE OF TRUTH KEYWORDS: GraphicsAdapters, GPU port, list GPUs, DirectX 12 adapters, DirectML devices, high-performance GPU first
 * WHAT:  GraphicsAdapters lists the hardware graphics adapters that can run DirectX 12 compute (what DirectML
 *        needs), the most capable first.
 * WHY:   `auto` only tries the GPU when a DX12 adapter exists, and remembers its benchmark per GPU and driver
 *        (02 §8.1, 05 A6); asking Windows which adapters exist is a Windows call, so it sits behind a port (root
 *        CLAUDE.md §3) and the accelerator rules stay testable with a fake. The list is read at every engine load,
 *        never cached, because a GPU can be plugged in, disabled or get a new driver while Echo runs. Software
 *        rasterizers (WARP, the Basic Render Driver) are never listed: DirectML refuses them and they are always
 *        slower than the CPU path. Blocking, called on the ASR loader thread.
 * WHERE: Implemented by adapters/gpu (DxgiGraphicsAdapters) and ports/fakes; read by pipeline/asr/accelerator.rs
 *        (AcceleratorPicker), which app/bootstrap builds with the adapter.
 */

use crate::types::{GpuAdapter, PortResult};

/// The DirectX 12 GPUs of this machine.
pub trait GraphicsAdapters: Send + Sync {
    /// Hardware adapters that can create a DirectX 12 device, the high-performance one first (the discrete GPU on a
    /// laptop that has two); empty when there is none.
    fn list(&self) -> PortResult<Vec<GpuAdapter>>;
}
