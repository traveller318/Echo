/*!
 * SOURCE OF TRUTH KEYWORDS: pipeline layer, session state machine, session actor, capture, ASR worker, polish chain, delivery, appearance
 * WHAT:  Layer 5: business orchestration of a take (session state machine, capture, ASR, polish, delivery,
 *        recovery, model management) and of app-wide derived state (appearance).
 * WHY:   The session actor is the sole owner of recording state (02 §5); nothing else may keep a copy.
 *        Works only through ports, never concrete adapters.
 * WHERE: Driven by app/ and ipc/commands; may import types/, ports/, services/ and registry/.
 */

pub mod appearance;
