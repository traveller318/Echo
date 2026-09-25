/*!
 * SOURCE OF TRUTH KEYWORDS: pipeline layer, session state machine, session actor, capture, ASR worker, polish chain, delivery, history actions, retry, pill presenter, crash recovery, retention sweep
 * WHAT:  Layer 5: business orchestration of a take (session state machine, capture, ASR, polish, delivery,
 *        model management), of History (delete, copy, paste-last, retry from the saved audio), of durability
 *        (startup recovery of takes a crash cut off, retention sweeps of old audio and rows), of the global
 *        hotkey bindings, of app-wide derived state (appearance), and of
 *        the pill window that follows the session (pill, fed through the event fan-out).
 * WHY:   The session actor is the sole owner of recording state (02 §5); nothing else may keep a copy.
 *        Works only through ports, never concrete adapters.
 * WHERE: Driven by app/ and ipc/commands; may import types/, ports/, services/ and registry/.
 */

pub mod appearance;
pub mod asr;
pub mod blocking;
pub mod capture;
pub mod delivery;
pub mod fan_out;
pub mod history;
pub mod hotkeys;
pub mod pill;
pub mod polish;
pub mod recovery;
pub mod retention;
pub mod retry;
pub mod session;
pub mod unwind;
