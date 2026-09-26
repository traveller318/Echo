/*!
 * SOURCE OF TRUTH KEYWORDS: command catalog, command groups, collect_commands, registered commands, IPC surface list
 * WHAT:  The command groups (one module each, 02 §4.3) and `catalog()`, the list of every command for the
 *        tauri-specta builder.
 * WHY:   One list feeds both the running app's invoke handler and the generated bindings (app/bindings.rs), so a
 *        command cannot be declared but left unreachable or untyped. Adding a command is an `echo_command!` in
 *        its group plus one path here.
 * WHERE: `catalog()` is read by app/bindings.rs; groups are filled step by step (session reads the take, sends
 *        the pill's stop, retries a stored take and sets the onboarding rehearsal; onboarding reads, completes and
 *        opens first-run setup; history lists, reads, copies, deletes, clears and pastes the last take;
 *        metrics computes the dashboard's summary and activity; models lists, downloads, cancels, imports,
 *        verifies, removes and activates models; engine reports where the speech engine runs and measures
 *        it again; audio lists devices and runs the
 *        microphone check; system holds appearance, the logs / privacy openers, the page opener and About; updates
 *        checks for and installs a newer Echo; hotkeys pauses Echo's hotkeys and reports it; pill takes the pill
 *        page's button areas and exit).
 */

pub mod audio;
pub mod engine;
pub mod history;
pub mod hotkeys;
pub mod metrics;
pub mod models;
pub mod onboarding;
pub mod pill;
pub mod session;
pub mod settings;
pub mod system;
pub mod updates;

use tauri::Runtime;
use tauri_specta::{Commands, collect_commands};

/// Every IPC command, ready for `tauri_specta::Builder::commands`.
pub fn catalog<R: Runtime>() -> Commands<R> {
    collect_commands![
        audio::audio_list_devices,
        audio::audio_test_level,
        session::session_get_state,
        session::session_input,
        session::session_retry,
        session::session_rehearse,
        history::history_list,
        history::history_get,
        history::history_copy,
        history::history_delete,
        history::history_clear,
        history::history_paste_last,
        metrics::metrics_summary,
        metrics::metrics_activity,
        engine::engine_status,
        engine::engine_remeasure,
        models::models_list,
        models::models_download,
        models::models_cancel_download,
        models::models_import,
        models::models_verify,
        models::models_remove,
        models::models_set_active,
        pill::pill_set_hit_areas,
        pill::pill_exited,
        settings::registry_get,
        settings::settings_availability,
        settings::settings_get_all,
        settings::settings_set,
        settings::settings_reset,
        system::appearance_get,
        system::app_open_logs_dir,
        system::app_open_mic_privacy_settings,
        system::app_open_page,
        system::app_about,
        updates::updates_check,
        updates::updates_install,
        hotkeys::hotkeys_status,
        hotkeys::hotkeys_pause,
        hotkeys::hotkeys_capture,
        onboarding::onboarding_get,
        onboarding::onboarding_complete,
        onboarding::onboarding_open,
    ]
}
