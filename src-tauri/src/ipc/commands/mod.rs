/*!
 * SOURCE OF TRUTH KEYWORDS: command catalog, command groups, collect_commands, registered commands, IPC surface list
 * WHAT:  The command groups (one module each, 02 §4.3) and `catalog()`, the list of every command for the
 *        tauri-specta builder.
 * WHY:   One list feeds both the running app's invoke handler and the generated bindings (app/bindings.rs), so a
 *        command cannot be declared but left unreachable or untyped. Adding a command is an `echo_command!` in
 *        its group plus one path here.
 * WHERE: `catalog()` is read by app/bindings.rs; groups are filled step by step (session, history, metrics,
 *        models and audio join with their layers; system holds appearance_get so far).
 */

pub mod settings;
pub mod system;

use tauri::Runtime;
use tauri_specta::{Commands, collect_commands};

/// Every IPC command, ready for `tauri_specta::Builder::commands`.
pub fn catalog<R: Runtime>() -> Commands<R> {
    collect_commands![
        settings::registry_get,
        settings::settings_get_all,
        settings::settings_set,
        settings::settings_reset,
        system::appearance_get,
    ]
}
