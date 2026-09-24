/*!
 * SOURCE OF TRUTH KEYWORDS: ipc layer, command factory, echo_command, CommandCtx, tauri commands, IPC boundary
 * WHAT:  Layer 6: the IPC boundary. The command factory plus thin command handlers grouped by feature.
 * WHY:   Every command is declared through the factory, which owns validation, permission preflight,
 *        reentrancy, tracing, error mapping and metrics (02 §4.1). `#[tauri::command]` appears nowhere else.
 * WHERE: Commands are registered by app/ through the tauri-specta builder; may import every layer except
 *        adapters/ and app/.
 */

pub mod commands;
mod context;
pub mod factory;
mod reentrancy;

pub use context::CommandCtx;
