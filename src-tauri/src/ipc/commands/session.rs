/*!
 * SOURCE OF TRUTH KEYWORDS: session commands, session_get_state, session_input, SessionView read, pill stop, SessionUiInput
 * WHAT:  The session command group (02 §4.3): `session_get_state` returns the SessionView of the current take;
 *        `session_input` sends a UI input (only `stop`, the pill's stop button) to the session actor.
 * WHY:   The UI reads the session once through this command and then stays fresh from SessionStateChanged (02 §4.4);
 *        the view comes from the actor, the sole owner of recording state, at the moment of the call. Recording
 *        starts and stops from hotkeys, not IPC: SessionUiInput holds only UI-originated inputs, so the UI cannot
 *        fake a hotkey, a timer or a worker reply. The input is accepted as soon as it is queued; whether it means
 *        anything in the current phase is the machine's decision, visible in the next SessionStateChanged.
 * WHERE: Registered through `ipc::commands::catalog`; called from the UI as `commands.sessionGetState()` and
 *        `commands.sessionInput("stop")` (the pill, step 15). `session_retry` joins with History (step 16).
 */

use crate::{
    ipc::{CommandCtx, factory::echo_command},
    types::{PortError, SessionUiInput, SessionView},
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

/// The session view now.
pub async fn get_state(ctx: &CommandCtx, (): ()) -> Result<SessionView, PortError> {
    ctx.session().view().await
}

/// Queues a pill input for the session actor.
pub async fn input(ctx: &CommandCtx, input: SessionUiInput) -> Result<(), PortError> {
    ctx.session().ui_input(input)
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
    }
}
