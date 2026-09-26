/*!
 * SOURCE OF TRUTH KEYWORDS: updates commands, updates_check, updates_install, UpdateStatus, Check for updates button, install update, Exclusive updates
 * WHAT:  The updates command group (02 §4.3): `updates_check` asks the update source for a newer Echo and
 *        `updates_install` installs the one it found.
 * WHY:   Both go through the Updater port and pipeline/updates.rs, so the rules (no source → `not_configured`
 *        without asking; no install while a take is in progress) live once. Both declare the Network permission,
 *        because a real update source goes online: offline mode refuses them at the factory like every download.
 *        They share one Exclusive key, so a second check or an install during a check is `Busy` at once (the automatic
 *        check takes the same key through `CommandCtx::hold`). Install also holds the History retry and model keys:
 *        the installer closes Echo, so a retry or a model transfer that is running makes it `Busy`, and none can
 *        start while it installs. This build
 *        has no update source (02 §11), so the UI hides the button from caps and shows why instead; the commands
 *        still answer calmly if asked.
 * WHERE: Registered through `ipc::commands::catalog`; called from Settings → About as `commands.updatesCheck()` /
 *        `commands.updatesInstall()` (src/hooks/use-updates.ts) when the updater caps say it is available.
 */

use crate::{
    ipc::{CommandCtx, factory::echo_command},
    pipeline::updates,
    types::{Permission, PortError, Reentrancy, UpdateStatus},
};

echo_command! {
    /// Looks for a newer Echo; `not_configured` when this build has no update source (nothing is asked).
    name: updates_check,
    output: UpdateStatus,
    permission: Some(Permission::Network),
    reentrancy: Exclusive("updates"),
    handler: check,
}

echo_command! {
    /// Installs the update the last check found; `Busy` while a take, a History retry or a model transfer is in
    /// progress, `NotFound { update }` when there is nothing to install.
    name: updates_install,
    output: (),
    permission: Some(Permission::Network),
    reentrancy: Exclusive("updates"),
    handler: install,
}

/// Asks the update source through the pipeline's caps guard.
pub async fn check(ctx: &CommandCtx, (): ()) -> Result<UpdateStatus, PortError> {
    updates::check(ctx.updater()).await
}

/// Installs the found update once no take, History retry or model transfer is in progress.
pub async fn install(ctx: &CommandCtx, (): ()) -> Result<(), PortError> {
    let _retry = ctx
        .locks()
        .acquire(Reentrancy::Exclusive("session_retry"))?;
    let _models = ctx.locks().acquire(Reentrancy::Exclusive("models"))?;
    let session = ctx.session().view().await?;
    updates::install(ctx.updater(), session.status).await
}

#[cfg(test)]
mod tests {
    use std::{thread, time::Duration};

    use super::*;
    use crate::{
        ipc::{factory, testing},
        ports::fakes::FakePrivacyConsent,
        registry::{self, settings::keys},
        types::{AppError, CommandSpec, Reentrancy, ResourceKind, SettingValue},
    };

    const CHECK: CommandSpec = CommandSpec {
        name: "updates_check",
        permission: Some(Permission::Network),
        reentrancy: Reentrancy::Exclusive("updates"),
    };

    const INSTALL: CommandSpec = CommandSpec {
        name: "updates_install",
        permission: Some(Permission::Network),
        reentrancy: Reentrancy::Exclusive("updates"),
    };

    fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(future)
    }

    fn online() -> testing::Harness {
        testing::harness(
            registry::settings::resolve([(keys::OFFLINE_MODE, SettingValue::Bool(false))]),
            FakePrivacyConsent::granted(),
        )
    }

    #[test]
    fn the_shipped_build_answers_not_configured_and_installs_nothing() {
        let harness = online();
        let actor = harness.session_actor;
        let runner = thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_time()
                .build()
                .unwrap()
                .block_on(actor.run());
        });

        assert_eq!(
            block_on(factory::run(&harness.ctx, &CHECK, (), check)),
            Ok(UpdateStatus::NotConfigured)
        );
        assert_eq!(
            block_on(factory::run(&harness.ctx, &INSTALL, (), install)),
            Err(AppError::NotFound {
                resource: ResourceKind::Update
            })
        );

        assert!(harness.ctx.session().shutdown(Duration::from_secs(5)));
        runner.join().unwrap();
    }

    #[test]
    fn offline_mode_refuses_both_before_the_updater_is_asked() {
        let harness = testing::harness(
            registry::settings::resolve([(keys::OFFLINE_MODE, SettingValue::Bool(true))]),
            FakePrivacyConsent::granted(),
        );
        let denied = AppError::Offline;
        assert_eq!(
            block_on(factory::run(&harness.ctx, &CHECK, (), check)),
            Err(denied.clone())
        );
        assert_eq!(
            block_on(factory::run(&harness.ctx, &INSTALL, (), install)),
            Err(denied)
        );
    }

    #[test]
    fn install_is_busy_while_a_retry_or_a_model_transfer_runs() {
        let harness = online();
        for key in ["session_retry", "models"] {
            let held = harness
                .ctx
                .locks()
                .acquire(Reentrancy::Exclusive(key))
                .unwrap();
            assert_eq!(
                block_on(factory::run(&harness.ctx, &INSTALL, (), install)),
                Err(AppError::Busy),
                "{key}"
            );
            drop(held);
        }
    }

    #[test]
    fn the_automatic_check_shares_the_commands_key() {
        let harness = online();
        let held = harness.ctx.hold(Reentrancy::Exclusive("updates")).unwrap();
        assert_eq!(
            block_on(factory::run(&harness.ctx, &CHECK, (), check)),
            Err(AppError::Busy)
        );
        drop(held);
        assert_eq!(
            block_on(factory::run(&harness.ctx, &CHECK, (), check)),
            Ok(UpdateStatus::NotConfigured)
        );
    }

    #[test]
    fn install_without_a_running_session_is_internal_not_a_hang() {
        let harness = online();
        drop(harness.session_actor);
        assert_eq!(
            block_on(factory::run(&harness.ctx, &INSTALL, (), install)),
            Err(AppError::Internal)
        );
    }
}
