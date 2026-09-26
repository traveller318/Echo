/*!
 * SOURCE OF TRUTH KEYWORDS: gpu adapters, GraphicsAdapters implementations, DxgiGraphicsAdapters, DirectX 12 GPU list
 * WHAT:  Adapters behind the GraphicsAdapters port.
 * WHY:   Asking Windows which GPUs exist is an operating-system call and stays behind its port (root CLAUDE.md §3).
 * WHERE: Constructed by app/bootstrap; used only through `dyn GraphicsAdapters` (pipeline/asr/accelerator.rs).
 */

mod dxgi;

pub use dxgi::DxgiGraphicsAdapters;
