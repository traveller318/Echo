/*!
 * SOURCE OF TRUTH KEYWORDS: session pipeline, session state machine, transition, session toasts, take lifecycle, sole owner of recording state
 * WHAT:  The session state machine of 02 §5: the pure `transition` (transition.rs) and the toasts it raises
 *        (notices.rs). Its state, inputs, effects and policy are data in types/session_machine.rs.
 * WHY:   The session is the sole owner of recording state; keeping the decisions in one pure function means the
 *        actor that will run them (actor.rs, step 14) only executes effects through ports and cannot disagree with
 *        the table. The data shapes live in types/ like every other shared type (root CLAUDE.md §4), so the actor,
 *        its tests and `session_get_state` all read the same definitions.
 * WHERE: `pipeline::session::transition` is called by the session actor for every inbox input.
 */

mod notices;
mod transition;

#[cfg(test)]
mod tests;

pub use notices::{
    DEVICE_LOST_TOAST, MAX_DURATION_TOAST, START_FAILED_TOAST, TAKE_FAILED_TOAST, stop_toast,
};
pub use transition::transition;
