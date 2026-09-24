/*!
 * SOURCE OF TRUTH KEYWORDS: session pipeline, session state machine, session actor, transition, effect runner, session toasts, take lifecycle, sole owner of recording state
 * WHAT:  The session of 02 §5: the pure `transition` (transition.rs) and the toasts it raises (notices.rs), and the
 *        actor that owns the state and runs the machine (actor.rs: SessionActor, SessionHandle, SessionConfig),
 *        with its inbox (inbox.rs), the hotkey → input mapping (hotkey_input.rs), the Arm effect (arm.rs) and the
 *        effect runner (runner.rs). The machine's state, inputs, effects and policy are data in
 *        types/session_machine.rs.
 * WHY:   The session is the sole owner of recording state; keeping the decisions in one pure function means the
 *        actor only executes effects through ports and cannot disagree with the table. The data shapes live in
 *        types/ like every other shared type (root CLAUDE.md §4), so the actor, its tests and `session_get_state`
 *        all read the same definitions. The actor is split by responsibility: the loop, the mailbox, the Arm (the
 *        one long blocking sequence) and the runner that holds a take's resources.
 * WHERE: app/bootstrap builds and spawns the actor; ipc/commands/session.rs talks to it through SessionHandle.
 */

mod actor;
mod arm;
mod hotkey_input;
mod inbox;
mod notices;
mod runner;
mod transition;

#[cfg(test)]
mod actor_tests;
#[cfg(test)]
mod tests;

pub use actor::{
    PolisherBuilder, SessionActor, SessionConfig, SessionEngines, SessionHandle, SessionInbox,
};
pub use arm::VadBuilder;
pub use notices::{
    DEVICE_LOST_TOAST, HOTKEY_UNAVAILABLE_TOAST, MAX_DURATION_TOAST, START_FAILED_TOAST,
    TAKE_FAILED_TOAST, stop_toast,
};
pub use transition::transition;
