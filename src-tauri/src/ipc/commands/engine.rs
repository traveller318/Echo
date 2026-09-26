/*!
 * SOURCE OF TRUTH KEYWORDS: engine commands, engine_status, engine_remeasure, SpeechEngineStatus, accelerator in use, measure again, About engine
 * WHAT:  The engine command group (02 §4.3): `engine_status` returns the speech engine's readiness and, once it is
 *        ready, where it runs, why and what the measurement found (SpeechEngineStatus); `engine_remeasure` forgets
 *        the selected engine's remembered accelerator measurements and, on `auto`, measures again now.
 * WHY:   The About section (step 25) shows the engine and the accelerator in use (02 §11) and the exit check of
 *        step 22 needs the warm-up times, so both come from the one status the ASR worker keeps; the UI reads it
 *        once and refetches on ModelsChanged, which the worker's readiness relay sends whenever the readiness or the
 *        running accelerator changes (never polled). Re-measuring is the escape hatch when the machine changed in a
 *        way the GPU fingerprint cannot see (a new CPU, a power profile): handlers only pick the pipeline call, the
 *        rule of what to forget and when to reload is pipeline/asr/switch.rs. It shares the Exclusive key `models`
 *        with the model transfers, so a measurement never races a download or removal of the model it loads; the
 *        database write runs on the blocking pool.
 * WHERE: Registered through `ipc::commands::catalog`; called by the About section (step 25) through
 *        `commands.engineStatus()` / `commands.engineRemeasure()` (src/hooks/use-speech-engine.ts).
 */

use crate::{
    ipc::{CommandCtx, factory::echo_command},
    pipeline::{asr, blocking::run_blocking},
    types::{AppError, PortError, SpeechEngineStatus},
};

echo_command! {
    /// The speech engine's readiness and, once it is ready, the accelerator it runs on, why, and the timings.
    name: engine_status,
    output: SpeechEngineStatus,
    permission: None,
    reentrancy: Shared,
    handler: status,
}

echo_command! {
    /// Forgets the remembered accelerator measurements of the selected engine; on automatic, measures again now.
    name: engine_remeasure,
    output: (),
    permission: None,
    reentrancy: Exclusive("models"),
    handler: remeasure,
}

/// The ASR worker's status as it is now.
pub async fn status(ctx: &CommandCtx, (): ()) -> Result<SpeechEngineStatus, AppError> {
    Ok(ctx.asr().status())
}

/// Forgets the measurements and asks for a reload when one would measure.
pub async fn remeasure(ctx: &CommandCtx, (): ()) -> Result<(), PortError> {
    let (worker, db, settings, paths) = (
        ctx.asr().clone(),
        ctx.db().clone(),
        ctx.settings(),
        ctx.paths().clone(),
    );
    run_blocking("forgetting accelerator measurements", move || {
        asr::remeasure(&worker, &db, &settings, &paths).map(drop)
    })
    .await
}

#[cfg(test)]
mod tests {
    use std::task::Poll;

    use super::*;
    use crate::{
        ipc::{factory, testing},
        ports::fakes::poll_once,
        types::{CommandSpec, Reentrancy},
    };

    #[test]
    fn the_status_is_unloaded_until_an_engine_is_ready() {
        let ctx = testing::ctx();
        let spec = CommandSpec {
            name: "engine_status",
            permission: None,
            reentrancy: Reentrancy::Shared,
        };
        let result = poll_once(factory::run(&ctx, &spec, (), status));
        assert_eq!(result, Poll::Ready(Ok(SpeechEngineStatus::UNLOADED)));
    }

    #[test]
    fn remeasure_on_an_unloaded_engine_only_forgets() {
        let ctx = testing::ctx();
        let spec = CommandSpec {
            name: "engine_remeasure",
            permission: None,
            reentrancy: Reentrancy::Exclusive("models"),
        };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let result = runtime.block_on(factory::run(&ctx, &spec, (), remeasure));
        assert_eq!(result, Ok(()));
        assert_eq!(ctx.asr().status(), SpeechEngineStatus::UNLOADED);
    }
}
