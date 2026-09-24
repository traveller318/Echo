/*!
 * SOURCE OF TRUTH KEYWORDS: EventSink, push events, port callback, hotkey events sink, power events sink, progress sink, capture events
 * WHAT:  EventSink<E>: where a port pushes events it produces on its own schedule (hotkey presses, power
 *        transitions, capture failures, model progress).
 * WHY:   A push sink keeps ports free of any channel or runtime choice: the pipeline decides whether an event goes
 *        into the session actor's inbox, a throttle or a test recorder, and the adapter never runs a forwarding
 *        thread. It replaces the `events() -> Receiver<…>` shape first sketched in 02 §3.4 (05 decision log).
 *        `emit` may be called from an OS callback thread (a hotkey hook, a power notification), so it must
 *        return quickly and never wait for the consumer.
 * WHERE: Taken by HotkeyService::listen, PowerEvents::listen, AudioCapture::start and ModelStore transfers, and
 *        by the capture worker for speech segments and its own failures (pipeline/capture); implemented by
 *        pipeline/ (actor inbox forwarders) and by ports/fakes RecordingSink in tests.
 */

/// Receives events a port produces. Implementations must not block.
pub trait EventSink<E>: Send + Sync {
    fn emit(&self, event: E);
}
