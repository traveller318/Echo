/*!
 * SOURCE OF TRUTH KEYWORDS: command catalog, command groups, collect_commands, registered commands, IPC surface list
 * WHAT:  The command groups (one module each, 02 §4.3) and `catalog()`, the list of every command for the
 *        tauri-specta builder.
 * WHY:   One list feeds both the running app's invoke handler and the generated bindings (app/bindings.rs), so a
 *        command cannot be declared but left unreachable or untyped. Adding a command is an `echo_command!` in
 *        its group plus one path here.
 * WHERE: `catalog()` is read by app/bindings.rs; groups are filled step by step (session reads the take and sends
 *        the pill's stop; history, metrics and models join with their layers; audio lists devices and runs the
 *        microphone check; system holds appearance, the logs / privacy openers and the page opener; pill takes the
 *        pill page's button areas and exit).
 */

pub mod audio;
pub mod pill;
pub mod session;
pub mod settings;
pub mod system;

use tauri::Runtime;
use tauri_specta::{Commands, collect_commands};

/// Every IPC command, ready for `tauri_specta::Builder::commands`.
pub fn catalog<R: Runtime>() -> Commands<R> {
    collect_commands![
        audio::audio_list_devices,
        audio::audio_test_level,
        session::session_get_state,
        session::session_input,
        pill::pill_set_hit_areas,
        pill::pill_exited,
        settings::registry_get,
        settings::settings_get_all,
        settings::settings_set,
        settings::settings_reset,
        system::appearance_get,
        system::app_open_logs_dir,
        system::app_open_mic_privacy_settings,
        system::app_open_page,
    ]
}
