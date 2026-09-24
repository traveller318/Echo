/*!
 * SOURCE OF TRUTH KEYWORDS: main.rs, binary entry, windows_subsystem, process exit code
 * WHAT:  Binary entry point: hands control to the composition root and returns its exit code.
 * WHY:   Kept to one call so every wiring decision lives in app/. The release build uses the Windows GUI
 *        subsystem so no console window opens next to Echo.
 * WHERE: Built as the `echo` executable; calls `echo_lib::app::run`.
 */
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> std::process::ExitCode {
    echo_lib::app::run()
}
