/*!
 * SOURCE OF TRUTH KEYWORDS: command factory, echo_command, factory run, validation, permission preflight, reentrancy guard, error mapping, command metric, tauri::command
 * WHAT:  The `echo_command!` macro (the only `#[tauri::command]` in the app) and `run`, the pipeline every command
 *        call goes through: span → validate → permission → reentrancy → handler → error mapping → metric.
 * WHY:   Cross-cutting concerns live once, here (02 §4.1), so a handler holds only its task logic and can never
 *        forget a check. `run` is a plain async fn over `&CommandCtx`, so tests drive it with fake handlers and
 *        port fakes, no Tauri app needed; the macro only adapts Tauri's `State` and registers the fn with specta.
 *        Handlers return `Result<O, E>` with `E: Into<PortError>`: an `AppError` passes through, a `PortError`
 *        has its log-only detail written here and dropped before the UI sees it. A panicking handler becomes
 *        `AppError::Internal` instead of a promise that never settles, and its reentrancy key is still released.
 *        Tracing records the command name and a per-process request id, never the input: inputs carry
 *        transcript text and dictionary entries (02 §10, §12).
 * WHERE: `echo_command!` is invoked in ipc/commands; the generated fns are listed by
 *        `ipc::commands::catalog` for the tauri-specta builder (app/bindings.rs).
 */

use std::{
    future::Future,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

use garde::Validate;
use tracing::Instrument;

use super::CommandCtx;
use crate::{
    pipeline::unwind::catch_unwind,
    registry::{
        metrics::COMMAND_DURATION,
        permissions::{self, PermissionCtx},
    },
    types::{AppError, CommandSpec, Permission, PortError},
};

/// Field name reported when a validation error is about the whole input rather than one field.
const WHOLE_INPUT: &str = "input";

/// Request ids are unique per process, so every line of one call can be found in the log.
static NEXT_REQUEST: AtomicU64 = AtomicU64::new(1);

/**
 * SOURCE OF TRUTH KEYWORDS: echo_command macro, tauri::command, specta::specta, command declaration, no-input command
 * WHAT:  Declares an IPC command: generates `pub async fn <name>` with `#[tauri::command]` + `#[specta::specta]`
 *        that runs `handler` through `run` with the declared input, permission and reentrancy.
 * WHY:   The one place `#[tauri::command]` is written (enforced by tests/architecture.rs). Declarations read like
 *        02 §4.1; `input:` may be omitted for a command that takes nothing (the handler then receives `()`).
 *        Doc comments above `name:` become the command's doc comment and reach the generated bindings.ts.
 *        `permission` is an expression (`None` or `Some(Permission::…)`); `reentrancy` is `Shared` or
 *        `Exclusive("key")`.
 * WHERE: ipc/commands/<group>.rs, e.g.
 *        `echo_command! { name: history_list, input: HistoryListInput, output: Page<TranscriptSummary>,
 *        permission: None, reentrancy: Shared, handler: list }`.
 */
macro_rules! echo_command {
    (
        $(#[$meta:meta])*
        name: $name:ident,
        input: $input:ty,
        output: $output:ty,
        permission: $permission:expr,
        reentrancy: $reentrancy:ident $(($key:literal))?,
        handler: $handler:path $(,)?
    ) => {
        $(#[$meta])*
        #[tauri::command]
        #[specta::specta]
        pub async fn $name(
            ctx: tauri::State<'_, $crate::ipc::CommandCtx>,
            input: $input,
        ) -> ::core::result::Result<$output, $crate::types::AppError> {
            const SPEC: $crate::types::CommandSpec = $crate::types::CommandSpec {
                name: ::core::stringify!($name),
                permission: $permission,
                reentrancy: $crate::types::Reentrancy::$reentrancy $(($key))?,
            };
            $crate::ipc::factory::run(ctx.inner(), &SPEC, input, $handler).await
        }
    };
    (
        $(#[$meta:meta])*
        name: $name:ident,
        output: $output:ty,
        permission: $permission:expr,
        reentrancy: $reentrancy:ident $(($key:literal))?,
        handler: $handler:path $(,)?
    ) => {
        $(#[$meta])*
        #[tauri::command]
        #[specta::specta]
        pub async fn $name(
            ctx: tauri::State<'_, $crate::ipc::CommandCtx>,
        ) -> ::core::result::Result<$output, $crate::types::AppError> {
            const SPEC: $crate::types::CommandSpec = $crate::types::CommandSpec {
                name: ::core::stringify!($name),
                permission: $permission,
                reentrancy: $crate::types::Reentrancy::$reentrancy $(($key))?,
            };
            $crate::ipc::factory::run(ctx.inner(), &SPEC, (), $handler).await
        }
    };
}
pub(crate) use echo_command;

/**
 * SOURCE OF TRUTH KEYWORDS: factory run, command pipeline, tracing span, request id, command outcome
 * WHAT:  Runs one command call: opens the span, runs the checks and the handler (`execute`), then records the
 *        duration and outcome as the `command-duration` log metric inside the same span.
 * WHY:   The handler's lifetime is tied to `ctx` ('c), not higher-ranked, so a plain `async fn(&CommandCtx, I)`
 *        item fits and the resulting future stays `Send` for Tauri's async runtime.
 * WHERE: Called by every fn `echo_command!` generates; called directly by the tests below.
 */
pub async fn run<'c, I, O, E, H, F>(
    ctx: &'c CommandCtx,
    spec: &CommandSpec,
    input: I,
    handler: H,
) -> Result<O, AppError>
where
    I: Validate<Context = ()>,
    E: Into<PortError>,
    H: FnOnce(&'c CommandCtx, I) -> F,
    F: Future<Output = Result<O, E>>,
{
    let request = NEXT_REQUEST.fetch_add(1, Ordering::Relaxed);
    let span = tracing::info_span!("command", command = spec.name, request);
    let started = Instant::now();
    let result = execute(ctx, spec, input, handler)
        .instrument(span.clone())
        .await;
    span.in_scope(|| record(&result, started.elapsed()));
    result
}

/// Steps 2–6 of 02 §4.1, in order; any failing step ends the call before the handler runs.
async fn execute<'c, I, O, E, H, F>(
    ctx: &'c CommandCtx,
    spec: &CommandSpec,
    input: I,
    handler: H,
) -> Result<O, AppError>
where
    I: Validate<Context = ()>,
    E: Into<PortError>,
    H: FnOnce(&'c CommandCtx, I) -> F,
    F: Future<Output = Result<O, E>>,
{
    input
        .validate()
        .map_err(|report| validation_error(&report))?;
    preflight(ctx, spec.permission)?;
    let _guard = ctx.locks().acquire(spec.reentrancy)?;
    match catch_unwind(handler(ctx, input)).await {
        Some(Ok(output)) => Ok(output),
        Some(Err(error)) => Err(logged(error.into())),
        None => {
            tracing::error!("command handler panicked");
            Err(AppError::Internal)
        }
    }
}

/**
 * SOURCE OF TRUTH KEYWORDS: garde report, AppError::Validation, first validation error, field path
 * WHAT:  Turns a garde report into one `AppError::Validation` for its first error: the field path
 *        (`key`, `pairs[2].from`) and garde's message.
 * WHY:   The UI shows one message per form (its Zod schema catches the rest first), so the first error is enough;
 *        an error on the input as a whole is reported against `input`.
 * WHERE: execute (step 2).
 */
fn validation_error(report: &garde::Report) -> AppError {
    match report.iter().next() {
        Some((path, error)) => {
            let field = path.to_string();
            let field = if field.is_empty() {
                WHOLE_INPUT.to_owned()
            } else {
                field
            };
            AppError::validation(field, error.message())
        }
        None => AppError::validation(WHOLE_INPUT, "The request is not valid."),
    }
}

/**
 * SOURCE OF TRUTH KEYWORDS: permission preflight, PermissionDenied, registry permissions check, offline network denied
 * WHAT:  Asks the registry whether the command's permission holds with the current settings and consent; fails
 *        with the permission's registry denial error when it does not (`Offline` for Network,
 *        `PermissionDenied { permission }` otherwise).
 * WHY:   Step 3: offline mode makes Network fail here, before any handler could open a socket. A check that could
 *        not be answered (the consent store unreadable) returns the port's safe error, with its detail logged,
 *        rather than guessing either way.
 * WHERE: execute (step 3).
 */
fn preflight(ctx: &CommandCtx, permission: Option<Permission>) -> Result<(), AppError> {
    let Some(permission) = permission else {
        return Ok(());
    };
    let settings = ctx.settings();
    let state = permissions::check(
        permission,
        &PermissionCtx {
            settings: &settings,
            consent: ctx.consent(),
        },
    )
    .map_err(logged)?;
    if state.is_granted() {
        Ok(())
    } else {
        Err(permissions::denial(permission))
    }
}

/// Writes a failure's internal detail to the local log and returns only its user-safe error (step 6).
fn logged(error: PortError) -> AppError {
    if let Some(detail) = error.detail() {
        tracing::warn!(
            code = error.error().code().as_str(),
            detail,
            "command failed"
        );
    }
    error.into_app_error()
}

/// Step 7: one `command-duration` log line with the outcome (`ok` or the AppError code) and milliseconds.
fn record<O>(result: &Result<O, AppError>, elapsed: Duration) {
    let outcome = match result {
        Ok(_) => "ok",
        Err(error) => error.code().as_str(),
    };
    tracing::info!(
        metric = COMMAND_DURATION.id.as_str(),
        outcome,
        duration_ms = elapsed.as_secs_f64() * 1000.0,
        "command finished"
    );
}

#[cfg(test)]
mod tests {
    use std::{
        io,
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, Ordering},
        },
        task::{Context, Poll, Waker},
    };

    use super::*;
    use crate::{
        ipc::testing,
        ports::fakes::{FakePrivacyConsent, poll_once},
        registry::settings::{self, keys},
        types::{PermissionState, Reentrancy, SettingValue, SettingsSnapshot},
    };

    #[derive(Debug, Validate)]
    struct RenameInput {
        #[garde(length(chars, min = 1, max = 40))]
        name: String,
    }

    fn rename(name: &str) -> RenameInput {
        RenameInput {
            name: name.to_owned(),
        }
    }

    const fn spec(permission: Option<Permission>, reentrancy: Reentrancy) -> CommandSpec {
        CommandSpec {
            name: "test_command",
            permission,
            reentrancy,
        }
    }

    const SHARED: CommandSpec = spec(None, Reentrancy::Shared);
    const EXCLUSIVE: CommandSpec = spec(None, Reentrancy::Exclusive("test_key"));

    fn ctx_with(settings: SettingsSnapshot, consent: FakePrivacyConsent) -> CommandCtx {
        testing::harness(settings, consent).ctx
    }

    fn ctx() -> CommandCtx {
        testing::ctx()
    }

    async fn echo_name(_: &CommandCtx, input: RenameInput) -> Result<String, AppError> {
        Ok(input.name)
    }

    /// Whether a running call holds EXCLUSIVE's key (taking it succeeds only when nobody does).
    fn held(ctx: &CommandCtx) -> bool {
        ctx.locks().acquire(EXCLUSIVE.reentrancy).is_err()
    }

    /// Runs a call whose handler never waits, so one poll finishes it.
    fn finish<O>(call: impl Future<Output = Result<O, AppError>>) -> Result<O, AppError> {
        match poll_once(call) {
            Poll::Ready(result) => result,
            Poll::Pending => panic!("the call did not finish on its first poll"),
        }
    }

    /// Collects everything the factory logs while `body` runs.
    fn captured_log(body: impl FnOnce()) -> String {
        #[derive(Clone, Default)]
        struct Buffer(Arc<Mutex<Vec<u8>>>);
        impl io::Write for Buffer {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let buffer = Buffer::default();
        let writer = buffer.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_writer(move || writer.clone())
            .finish();
        tracing::subscriber::with_default(subscriber, body);
        let bytes = buffer.0.lock().unwrap().clone();
        String::from_utf8(bytes).unwrap()
    }

    #[test]
    fn a_valid_call_reaches_the_handler() {
        let ctx = ctx();
        assert_eq!(
            finish(run(&ctx, &SHARED, rename("Echo"), echo_name)),
            Ok("Echo".to_owned())
        );
    }

    #[test]
    fn invalid_input_is_a_validation_error_and_the_handler_never_runs() {
        let ctx = ctx();
        let ran = AtomicBool::new(false);
        let result = finish(run(&ctx, &SHARED, rename(""), |_, _| async {
            ran.store(true, Ordering::SeqCst);
            Ok::<_, AppError>(())
        }));
        let Err(AppError::Validation { field, message }) = result else {
            panic!("expected a validation error, got {result:?}");
        };
        assert_eq!(field, "name");
        assert!(!message.is_empty());
        assert!(!ran.load(Ordering::SeqCst));
    }

    #[test]
    fn offline_mode_denies_network_commands_before_the_handler() {
        let network = spec(Some(Permission::Network), Reentrancy::Shared);
        let offline = ctx_with(
            settings::resolve([(keys::OFFLINE_MODE, SettingValue::Bool(true))]),
            FakePrivacyConsent::granted(),
        );
        let ran = AtomicBool::new(false);
        let result = finish(run(&offline, &network, (), |_, ()| async {
            ran.store(true, Ordering::SeqCst);
            Ok::<_, AppError>(())
        }));
        assert_eq!(result, Err(AppError::Offline));
        assert!(!ran.load(Ordering::SeqCst));

        let online = ctx();
        assert_eq!(
            finish(run(&online, &network, (), |_, ()| async {
                Ok::<_, AppError>(())
            })),
            Ok(())
        );
    }

    #[test]
    fn preflight_reads_the_current_settings_snapshot() {
        let network = spec(Some(Permission::Network), Reentrancy::Shared);
        let ctx = ctx();
        ctx.shared_settings().replace(settings::resolve([(
            keys::OFFLINE_MODE,
            SettingValue::Bool(true),
        )]));
        assert_eq!(
            finish(run(&ctx, &network, (), |_, ()| async {
                Ok::<_, AppError>(())
            })),
            Err(AppError::Offline)
        );
    }

    #[test]
    fn microphone_permission_follows_consent_and_an_unreadable_consent_is_not_guessed() {
        let microphone = spec(Some(Permission::Microphone), Reentrancy::Shared);
        let ctx = ctx_with(
            settings::defaults(),
            FakePrivacyConsent::new(PermissionState::Denied),
        );
        assert_eq!(
            finish(run(&ctx, &microphone, (), |_, ()| async {
                Ok::<_, AppError>(())
            })),
            Err(AppError::PermissionDenied {
                permission: Permission::Microphone
            })
        );

        let consent = FakePrivacyConsent::granted();
        consent
            .fail_next(PortError::new(AppError::AudioDevice).with_detail("consent store locked"));
        let ctx = ctx_with(settings::defaults(), consent);
        assert_eq!(
            finish(run(&ctx, &microphone, (), |_, ()| async {
                Ok::<_, AppError>(())
            })),
            Err(AppError::AudioDevice)
        );
    }

    #[test]
    fn an_exclusive_command_is_busy_while_one_runs_and_free_after_it_is_dropped() {
        let ctx = ctx();
        let mut running = Box::pin(run(&ctx, &EXCLUSIVE, (), |_, ()| async {
            std::future::pending::<Result<(), AppError>>().await
        }));
        let mut cx = Context::from_waker(Waker::noop());
        assert!(running.as_mut().poll(&mut cx).is_pending());
        assert!(held(&ctx));

        assert_eq!(
            finish(run(&ctx, &EXCLUSIVE, (), |_, ()| async {
                Ok::<_, AppError>(())
            })),
            Err(AppError::Busy)
        );
        assert_eq!(
            finish(run(&ctx, &SHARED, (), |_, ()| async {
                Ok::<_, AppError>(())
            })),
            Ok(()),
            "a shared command is not blocked by an exclusive key"
        );

        drop(running);
        assert!(!held(&ctx), "cancelling releases the key");
        assert_eq!(
            finish(run(&ctx, &EXCLUSIVE, (), |_, ()| async {
                Ok::<_, AppError>(())
            })),
            Ok(())
        );
    }

    #[test]
    fn internal_detail_is_logged_but_never_reaches_the_ui() {
        let ctx = ctx();
        let detail = "SQLITE_BUSY: database is locked (secret-internal-detail)";
        let mut result = None;
        let log = captured_log(|| {
            result = Some(finish(run(&ctx, &SHARED, (), |_, ()| async {
                Err::<(), _>(PortError::new(AppError::Storage).with_detail(detail))
            })));
        });
        let error = result.unwrap().unwrap_err();
        assert_eq!(error, AppError::Storage);
        let wire = serde_json::to_string(&error).unwrap();
        assert!(!wire.contains("secret-internal-detail"), "{wire}");
        assert!(!error.to_string().contains("secret-internal-detail"));
        assert!(
            log.contains("secret-internal-detail"),
            "the detail goes to the log: {log}"
        );
    }

    #[test]
    fn a_panicking_handler_is_an_internal_error_and_releases_its_key() {
        let ctx = ctx();
        let result = finish(run(&ctx, &EXCLUSIVE, (), |_, ()| async {
            if held(&ctx) {
                panic!("handler failure");
            }
            Ok::<(), AppError>(())
        }));
        assert_eq!(result, Err(AppError::Internal));
        assert!(!held(&ctx));
    }

    #[test]
    fn every_call_records_its_outcome_without_the_payload() {
        let ctx = ctx();
        let log = captured_log(|| {
            finish(run(&ctx, &SHARED, rename("secret-payload-text"), echo_name)).unwrap();
            finish(run(&ctx, &SHARED, rename(""), echo_name)).unwrap_err();
        });
        let lines: Vec<&str> = log
            .lines()
            .filter(|line| line.contains("command-duration"))
            .collect();
        assert_eq!(lines.len(), 2, "{log}");
        assert!(lines[0].contains("outcome=\"ok\"") && lines[0].contains("duration_ms="));
        assert!(lines[1].contains("outcome=\"Validation\""), "{}", lines[1]);
        assert!(
            lines
                .iter()
                .all(|line| line.contains("command=\"test_command\""))
        );
        assert!(!log.contains("secret-payload-text"), "{log}");
    }

    #[test]
    fn validation_errors_name_the_field_or_the_whole_input() {
        let mut report = garde::Report::new();
        report.append(garde::Path::empty(), garde::Error::new("must not be empty"));
        assert_eq!(
            validation_error(&report),
            AppError::validation("input", "must not be empty")
        );
        let mut nested = garde::Report::new();
        nested.append(
            garde::Path::new("pairs").join(2_usize).join("from"),
            garde::Error::new("too long"),
        );
        let AppError::Validation { field, .. } = validation_error(&nested) else {
            panic!("expected a validation error");
        };
        assert_eq!(field, "pairs[2].from");
        assert_eq!(
            validation_error(&garde::Report::new()),
            AppError::validation("input", "The request is not valid.")
        );
    }
}
