/*!
 * SOURCE OF TRUTH KEYWORDS: session commands, session_get_state, session_input, session_retry, session_rehearse, SessionView read, pill stop, retry take, rehearsal
 * WHAT:  The session command group (02 §4.3): `session_get_state` returns the SessionView of the current take;
 *        `session_input` sends a UI input (only `stop`, the pill's stop button) to the session actor;
 *        `session_retry` re-runs a stored take from its saved audio and returns its updated History row;
 *        `session_rehearse` sets what the session rehearses while an Echo window has focus (onboarding).
 * WHY:   The UI reads the session once through this command and then stays fresh from SessionStateChanged (02 §4.4);
 *        the view comes from the actor, the sole owner of recording state, at the moment of the call. Recording
 *        starts and stops from hotkeys, not IPC: SessionUiInput holds only UI-originated inputs, so the UI cannot
 *        fake a hotkey, a timer or a worker reply. The input is accepted as soon as it is queued; whether it means
 *        anything in the current phase is the machine's decision, visible in the next SessionStateChanged.
 *        Retry is Exclusive (`session_retry`): the ASR thread is shared with live takes, so one retry at a time
 *        keeps a take's stop → paste budget (02 §6.2); a second click gets `Busy` at once. It reads the session's
 *        view first so the take still in progress is refused; the work is pipeline/retry.rs. A rehearsal changes
 *        what a hotkey or a delivery means only while Echo itself has focus (pipeline/session/rehearsal.rs), so the
 *        UI may set it freely and a window that forgets to turn it off cannot break dictation elsewhere.
 * WHERE: Registered through `ipc::commands::catalog`; called from the UI as `commands.sessionGetState()` and
 *        `commands.sessionInput("stop")` (the pill, step 15), `commands.sessionRetry({ id })` (History, step 16),
 *        `commands.sessionRehearse("take" | "off")` (onboarding's practice step; `hotkey` stays for a press-only test).
 */

use crate::{
    ipc::{CommandCtx, factory::echo_command},
    pipeline::retry,
    types::{
        PortError, SessionRehearsal, SessionUiInput, SessionView, TranscriptInput,
        TranscriptSummary,
    },
};

echo_command! {
    /// The current take as the pill renders it; Idle when no take is running.
    name: session_get_state,
    output: SessionView,
    permission: None,
    reentrancy: Shared,
    handler: get_state,
}

echo_command! {
    /// Sends a pill input (`stop`) to the current take.
    name: session_input,
    input: SessionUiInput,
    output: (),
    permission: None,
    reentrancy: Shared,
    handler: input,
}

echo_command! {
    /// Transcribes a stored take again from its saved audio and returns its updated History row.
    name: session_retry,
    input: TranscriptInput,
    output: TranscriptSummary,
    permission: None,
    reentrancy: Exclusive("session_retry"),
    handler: retry_take,
}

echo_command! {
    /// Sets what the session rehearses while an Echo window has focus: `hotkey` reports presses without starting a
    /// take (HotkeyRehearsed), `take` shows a take's text in Echo instead of pasting it, `off` ends it.
    name: session_rehearse,
    input: SessionRehearsal,
    output: (),
    permission: None,
    reentrancy: Shared,
    handler: rehearse,
}

/// The session view now.
pub async fn get_state(ctx: &CommandCtx, (): ()) -> Result<SessionView, PortError> {
    ctx.session().view().await
}

/// Queues a pill input for the session actor.
pub async fn input(ctx: &CommandCtx, input: SessionUiInput) -> Result<(), PortError> {
    ctx.session().ui_input(input)
}

/// Queues the rehearsal for the session actor.
pub async fn rehearse(ctx: &CommandCtx, rehearsal: SessionRehearsal) -> Result<(), PortError> {
    ctx.session().rehearse(rehearsal)
}

/// Retries a stored take unless the session is still working on it.
pub async fn retry_take(
    ctx: &CommandCtx,
    input: TranscriptInput,
) -> Result<TranscriptSummary, PortError> {
    let session = ctx.session().view().await?;
    retry::retry(&ctx.retry_deps(), ctx.settings(), &session, input.id).await
}

#[cfg(test)]
mod tests {
    use std::thread;

    use super::*;
    use crate::{
        ipc::{factory, testing},
        ports::fakes::FakePrivacyConsent,
        registry,
        types::{AppError, CommandSpec, Reentrancy, SessionStatus},
    };

    const GET_STATE: CommandSpec = CommandSpec {
        name: "session_get_state",
        permission: None,
        reentrancy: Reentrancy::Shared,
    };

    const INPUT: CommandSpec = CommandSpec {
        name: "session_input",
        permission: None,
        reentrancy: Reentrancy::Shared,
    };

    fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(future)
    }

    #[test]
    fn the_state_comes_from_the_running_actor_and_a_stop_while_idle_changes_nothing() {
        let harness = testing::harness(
            registry::settings::defaults(),
            FakePrivacyConsent::granted(),
        );
        let actor = harness.session_actor;
        let runner = thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_time()
                .build()
                .unwrap()
                .block_on(actor.run());
        });

        let view = block_on(factory::run(&harness.ctx, &GET_STATE, (), get_state)).unwrap();
        assert_eq!(view, SessionView::IDLE);
        block_on(factory::run(
            &harness.ctx,
            &INPUT,
            SessionUiInput::Stop,
            input,
        ))
        .unwrap();
        let view = block_on(factory::run(&harness.ctx, &GET_STATE, (), get_state)).unwrap();
        assert_eq!(view.status, SessionStatus::Idle);

        assert!(
            harness
                .ctx
                .session()
                .shutdown(std::time::Duration::from_secs(5))
        );
        runner.join().unwrap();
    }

    #[test]
    fn a_stopped_actor_is_an_internal_error_not_a_hang() {
        let harness = testing::harness(
            registry::settings::defaults(),
            FakePrivacyConsent::granted(),
        );
        drop(harness.session_actor);
        assert_eq!(
            block_on(factory::run(&harness.ctx, &GET_STATE, (), get_state)),
            Err(AppError::Internal)
        );
        assert_eq!(
            block_on(factory::run(
                &harness.ctx,
                &INPUT,
                SessionUiInput::Stop,
                input
            )),
            Err(AppError::Internal)
        );
        assert_eq!(
            block_on(factory::run(
                &harness.ctx,
                &REHEARSE,
                SessionRehearsal::Hotkey,
                rehearse
            )),
            Err(AppError::Internal)
        );
    }

    const REHEARSE: CommandSpec = CommandSpec {
        name: "session_rehearse",
        permission: None,
        reentrancy: Reentrancy::Shared,
    };

    #[test]
    fn a_rehearsal_is_queued_for_the_actor() {
        let harness = testing::harness(
            registry::settings::defaults(),
            FakePrivacyConsent::granted(),
        );
        for rehearsal in [
            SessionRehearsal::Hotkey,
            SessionRehearsal::Take,
            SessionRehearsal::Off,
        ] {
            assert_eq!(
                block_on(factory::run(&harness.ctx, &REHEARSE, rehearsal, rehearse)),
                Ok(())
            );
        }
    }
}
