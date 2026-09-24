/*!
 * SOURCE OF TRUTH KEYWORDS: PowerEvent, suspend, resume, sleep, wake, power notification
 * WHAT:  PowerEvent: the machine is about to sleep, or has woken up.
 * WHY:   Suspend must finalize an active take before the machine sleeps, and resume must re-register hotkeys
 *        (Windows can drop them, 05 W8) and reopen audio lazily (02 §9). Only these two transitions change what
 *        Echo does, so the enum stays that small.
 * WHERE: `PowerEvents::listen` (ports/power.rs); the session actor handles Suspend, the pipeline's hotkey wiring
 *        handles Resume.
 */

/// A system power transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PowerEvent {
    Suspend,
    Resume,
}
