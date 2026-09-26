/*!
 * SOURCE OF TRUTH KEYWORDS: launch registry, LOGIN_ARG, --minimized, LAUNCH_AT_LOGIN_ARGS, STARTUP_IDLE_DELAY, lazy model load, start at sign-in arguments
 * WHAT:  How Echo is launched: the argument its start-at-sign-in entry passes (`--minimized`, LOGIN_ARG), the full
 *        argument list that entry carries, and how long after startup the heavy warm-up waits (STARTUP_IDLE_DELAY).
 * WHY:   The Run entry that writes the argument and the startup code that reads it must agree, so the string lives
 *        once. W19 (05): a start at sign-in happens while Windows is still bringing up the shell and audio, so the
 *        speech engine, the voice detector and the polish chain load after the machine had a moment to settle;
 *        hotkeys bind at once, and a take that comes earlier loads what it needs itself.
 * WHERE: app/bootstrap (the Win32RunKey arguments, the warm-up delay), pipeline/launch.rs (`origin_from_args`).
 */

use std::time::Duration;

/// The argument the start-at-sign-in entry passes; its presence means Windows started Echo at sign-in.
pub const LOGIN_ARG: &str = "--minimized";

/// Every argument the start-at-sign-in entry passes.
pub const LAUNCH_AT_LOGIN_ARGS: &[&str] = &[LOGIN_ARG];

/// How long after the windows exist the speech engine, voice detector and polish chain start loading.
pub const STARTUP_IDLE_DELAY: Duration = Duration::from_secs(2);
