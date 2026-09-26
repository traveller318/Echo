/*!
 * SOURCE OF TRUTH KEYWORDS: hotkeys commands, hotkeys_status, hotkeys_pause, hotkeys_capture, pause hotkeys, capture lease, HotkeyStatus
 * WHAT:  The hotkeys command group: `hotkeys_status` returns whether Echo's always-on hotkeys are off and why;
 *        `hotkeys_pause` switches the user's pause (the same switch as the tray's "Pause hotkeys");
 *        `hotkeys_capture` tells Echo a hotkey field started or stopped capturing a combination.
 * WHY:   The state lives in the one HotkeyGate the tray and the session share (pipeline/hotkey_gate.rs), so Settings,
 *        the tray and a second window always agree; the UI reads it once and follows HotkeyStatusChanged. While a
 *        field captures, the keyboard hook is off, so the field can read the chord Echo itself is bound to and
 *        pressing the current dictation chord starts no take (05 §6 item resolved in step 25); the gate ends a lease
 *        a page forgot after a minute.
 * WHERE: Registered through `ipc::commands::catalog`; called from src/hooks/use-hotkey-status.ts (status, pause)
 *        and src/components/global/hotkey-input (capture).
 */

use crate::{
    ipc::{CommandCtx, factory::echo_command},
    types::{AppError, HotkeyCaptureInput, HotkeyStatus, HotkeysPauseInput},
};

echo_command! {
    /// Whether Echo's hotkeys are paused by the user, or off while a hotkey field captures a combination.
    name: hotkeys_status,
    output: HotkeyStatus,
    permission: None,
    reentrancy: Shared,
    handler: status,
}

echo_command! {
    /// Pauses (`paused: true`) or resumes Echo's hotkeys, like the tray's "Pause hotkeys"; returns the new status.
    name: hotkeys_pause,
    input: HotkeysPauseInput,
    output: HotkeyStatus,
    permission: None,
    reentrancy: Shared,
    handler: pause,
}

echo_command! {
    /// A hotkey field started (`active: true`) or stopped capturing: Echo's hotkeys are off meanwhile (for at most a
    /// minute without a new start); returns the new status.
    name: hotkeys_capture,
    input: HotkeyCaptureInput,
    output: HotkeyStatus,
    permission: None,
    reentrancy: Shared,
    handler: capture,
}

/// The gate's status now.
pub async fn status(ctx: &CommandCtx, (): ()) -> Result<HotkeyStatus, AppError> {
    Ok(ctx.hotkey_gate().status())
}

/// Switches the user's pause.
pub async fn pause(ctx: &CommandCtx, input: HotkeysPauseInput) -> Result<HotkeyStatus, AppError> {
    Ok(ctx.hotkey_gate().set_paused(input.paused))
}

/// Starts or ends a capture lease.
pub async fn capture(
    ctx: &CommandCtx,
    input: HotkeyCaptureInput,
) -> Result<HotkeyStatus, AppError> {
    Ok(ctx.hotkey_gate().capture(input.active))
}

#[cfg(test)]
mod tests {
    use std::task::Poll;

    use super::*;
    use crate::{
        ipc::{factory, testing},
        ports::fakes::poll_once,
        registry::hotkeys::RECORD,
        types::{AppEvent, CommandSpec, HotkeyStatusChanged, Reentrancy},
    };

    const fn spec(name: &'static str) -> CommandSpec {
        CommandSpec {
            name,
            permission: None,
            reentrancy: Reentrancy::Shared,
        }
    }

    #[test]
    fn pause_and_capture_switch_the_shared_gate_and_announce_it() {
        let harness = testing::harness(
            crate::registry::settings::defaults(),
            crate::ports::fakes::FakePrivacyConsent::granted(),
        );
        let ctx = &harness.ctx;
        assert!(ctx.hotkey_gate().bind());
        assert!(harness.hotkeys.binding(&RECORD).is_some());

        let Poll::Ready(Ok(paused)) = poll_once(factory::run(
            ctx,
            &spec("hotkeys_pause"),
            HotkeysPauseInput { paused: true },
            pause,
        )) else {
            panic!("hotkeys_pause did not answer");
        };
        assert!(paused.paused && !paused.capturing);
        assert_eq!(harness.hotkeys.binding(&RECORD), None);

        let Poll::Ready(Ok(capturing)) = poll_once(factory::run(
            ctx,
            &spec("hotkeys_capture"),
            HotkeyCaptureInput { active: false },
            capture,
        )) else {
            panic!("hotkeys_capture did not answer");
        };
        assert_eq!(
            capturing, paused,
            "ending a capture never resumes the user's pause"
        );

        let Poll::Ready(Ok(read)) =
            poll_once(factory::run(ctx, &spec("hotkeys_status"), (), status))
        else {
            panic!("hotkeys_status did not answer");
        };
        assert_eq!(read, paused);
        assert!(
            harness
                .events
                .events()
                .contains(&AppEvent::HotkeyStatusChanged(HotkeyStatusChanged(paused)))
        );
    }
}
