/*!
 * SOURCE OF TRUTH KEYWORDS: models commands, models_list, models_download, models_cancel_download, models_import, models_verify, models_remove, models_set_active, Permission::Network, model transfer
 * WHAT:  The models command group (02 §4.3): list the Models page view, download (Network), cancel a running
 *        transfer, import from a folder the user picks, verify, remove, and make an engine the one in use.
 * WHY:   Handlers only pick the ModelManager call; orchestration lives in pipeline/models. Downloads declare
 *        `Permission::Network`, so offline mode refuses them at the factory before any socket opens (02 §10); the
 *        HTTP client's gate stops one already running. Download, import, verify and remove share the Exclusive
 *        key `models`, so two transfers never race over the same folders (a second is `Busy` at once); cancel is
 *        Shared because it must reach the transfer that holds that key. `models_set_active` writes the settings
 *        the registry entry names through the settings write path (validation, SettingsChanged, the live engine
 *        swap of 02 §8.1), so switching engines has no second code path. The list reads disk sizes, so it runs on
 *        the blocking pool, as does removal.
 * WHERE: Registered through `ipc::commands::catalog`; called by the Models page (routes/models) and onboarding.
 */

use crate::{
    ipc::{CommandCtx, commands::settings, factory::echo_command},
    pipeline::blocking::run_blocking,
    types::{
        EngineInput, ModelInput, ModelTransferOutcome, ModelsView, Permission, PortError,
        SettingsSetInput,
    },
};

echo_command! {
    /// Every engine that runs a model, with its status, selection, what it is doing and any running transfer,
    /// plus whether downloads may run now.
    name: models_list,
    output: ModelsView,
    permission: None,
    reentrancy: Shared,
    handler: list,
}

echo_command! {
    /// Downloads a model (resuming an earlier partial download), verifies and installs it. Progress arrives as
    /// ModelProgress events; `cancelled` when `models_cancel_download` stopped it.
    name: models_download,
    input: ModelInput,
    output: ModelTransferOutcome,
    permission: Some(Permission::Network),
    reentrancy: Exclusive("models"),
    handler: download,
}

echo_command! {
    /// Stops the model's running download, import or check; the partial download is kept.
    name: models_cancel_download,
    input: ModelInput,
    output: (),
    permission: None,
    reentrancy: Shared,
    handler: cancel,
}

echo_command! {
    /// Asks for a folder, then copies, verifies and installs the model from it (works offline); `cancelled` when
    /// the picker was closed.
    name: models_import,
    input: ModelInput,
    output: ModelTransferOutcome,
    permission: None,
    reentrancy: Exclusive("models"),
    handler: import,
}

echo_command! {
    /// Checks every file of the model against its SHA-256; `ModelCorrupt` when one does not match.
    name: models_verify,
    input: ModelInput,
    output: ModelTransferOutcome,
    permission: None,
    reentrancy: Exclusive("models"),
    handler: verify,
}

echo_command! {
    /// Deletes a downloaded model and its partial download.
    name: models_remove,
    input: ModelInput,
    output: (),
    permission: None,
    reentrancy: Exclusive("models"),
    handler: remove,
}

echo_command! {
    /// Makes an engine the one in use (its model must be installed); the new engine warms up while the old one
    /// keeps serving, and a take in progress finishes on the old one.
    name: models_set_active,
    input: EngineInput,
    output: (),
    permission: None,
    reentrancy: Shared,
    handler: set_active,
}

/// The Models page view.
pub async fn list(ctx: &CommandCtx, (): ()) -> Result<ModelsView, PortError> {
    let models = ctx.models().clone();
    run_blocking("listing models", move || models.list()).await
}

/// Downloads the model.
pub async fn download(
    ctx: &CommandCtx,
    input: ModelInput,
) -> Result<ModelTransferOutcome, PortError> {
    ctx.models().download(&input.model_id).await
}

/// Stops the model's running transfer, if any.
pub async fn cancel(ctx: &CommandCtx, input: ModelInput) -> Result<(), PortError> {
    if !ctx.models().cancel(&input.model_id) {
        tracing::debug!(model = %input.model_id, "nothing to cancel");
    }
    Ok(())
}

/// Imports the model from a folder the user picks.
pub async fn import(
    ctx: &CommandCtx,
    input: ModelInput,
) -> Result<ModelTransferOutcome, PortError> {
    ctx.models().import(&input.model_id).await
}

/// Re-hashes the model.
pub async fn verify(
    ctx: &CommandCtx,
    input: ModelInput,
) -> Result<ModelTransferOutcome, PortError> {
    ctx.models().verify(&input.model_id).await
}

/// Deletes the model.
pub async fn remove(ctx: &CommandCtx, input: ModelInput) -> Result<(), PortError> {
    let models = ctx.models().clone();
    run_blocking("removing a model", move || models.remove(&input.model_id)).await
}

/// Writes the settings that select the engine, each through the settings write path.
pub async fn set_active(ctx: &CommandCtx, input: EngineInput) -> Result<(), PortError> {
    for (key, value) in ctx.models().activation(&input.engine_id)? {
        settings::set(ctx, SettingsSetInput { key, value }).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::{
        ipc::{factory, testing},
        ports::{ModelStore, fakes::FakePrivacyConsent},
        registry::{
            engines::{PARAKEET_TDT_V3 as PARAKEET_ENGINE, SILERO_VAD},
            models::PARAKEET_TDT_V3,
            settings::{self as registry_settings, keys},
        },
        types::{AppError, AppEvent, CommandSpec, ModelId, ModelStatus, Reentrancy, SettingValue},
    };

    const DOWNLOAD: CommandSpec = CommandSpec {
        name: "models_download",
        permission: Some(Permission::Network),
        reentrancy: Reentrancy::Exclusive("models"),
    };

    const IMPORT: CommandSpec = CommandSpec {
        name: "models_import",
        permission: None,
        reentrancy: Reentrancy::Exclusive("models"),
    };

    const SHARED: CommandSpec = CommandSpec {
        name: "models_shared",
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

    fn parakeet() -> ModelInput {
        ModelInput {
            model_id: PARAKEET_TDT_V3,
        }
    }

    #[test]
    fn offline_mode_refuses_downloads_at_the_factory_but_import_still_works() {
        let harness = testing::harness(
            registry_settings::resolve([(keys::OFFLINE_MODE, SettingValue::Bool(true))]),
            FakePrivacyConsent::granted(),
        );
        assert_eq!(
            block_on(factory::run(&harness.ctx, &DOWNLOAD, parakeet(), download)),
            Err(AppError::Offline)
        );
        assert_eq!(
            block_on(factory::run(&harness.ctx, &SHARED, (), list))
                .unwrap()
                .entries[0]
                .status,
            ModelStatus::NotInstalled
        );
        harness.folder_picker.answer(Some(PathBuf::from("picked")));
        assert_eq!(
            block_on(factory::run(&harness.ctx, &IMPORT, parakeet(), import)),
            Ok(ModelTransferOutcome::Completed)
        );
        assert_eq!(
            harness
                .model_store
                .status(crate::registry::models::find(&PARAKEET_TDT_V3).unwrap())
                .unwrap(),
            ModelStatus::Installed
        );
    }

    #[test]
    fn download_installs_and_announces_progress_and_the_list() {
        let harness =
            testing::harness(registry_settings::defaults(), FakePrivacyConsent::granted());
        assert_eq!(
            block_on(factory::run(&harness.ctx, &DOWNLOAD, parakeet(), download)),
            Ok(ModelTransferOutcome::Completed)
        );
        let events = harness.events.events();
        assert!(
            events
                .iter()
                .any(|event| matches!(event, AppEvent::ModelProgress(_)))
        );
        assert!(
            events
                .iter()
                .any(|event| matches!(event, AppEvent::ModelsChanged(_)))
        );
        let view = block_on(factory::run(&harness.ctx, &SHARED, (), list)).unwrap();
        assert_eq!(view.entries[0].status, ModelStatus::Installed);
    }

    #[test]
    fn malformed_ids_are_validation_errors_and_unknown_ones_not_found() {
        let ctx = testing::ctx();
        let junk = ModelInput {
            model_id: ModelId::from("../evil".to_owned()),
        };
        assert!(matches!(
            block_on(factory::run(&ctx, &SHARED, junk, remove)),
            Err(AppError::Validation { ref field, .. }) if field == "model_id"
        ));
        let unknown = ModelInput {
            model_id: ModelId::from_static("unknown-model"),
        };
        assert!(matches!(
            block_on(factory::run(&ctx, &SHARED, unknown, cancel)),
            Ok(())
        ));
    }

    #[test]
    fn set_active_writes_the_engine_setting_once_its_model_is_installed() {
        let harness =
            testing::harness(registry_settings::defaults(), FakePrivacyConsent::granted());
        let input = EngineInput {
            engine_id: PARAKEET_ENGINE,
        };
        assert!(matches!(
            block_on(factory::run(
                &harness.ctx,
                &SHARED,
                input.clone(),
                set_active
            )),
            Err(AppError::ModelMissing { .. })
        ));
        harness.model_store.install(&PARAKEET_TDT_V3);
        assert_eq!(
            block_on(factory::run(&harness.ctx, &SHARED, input, set_active)),
            Ok(())
        );
        assert!(harness.events.events().iter().any(|event| matches!(
            event,
            AppEvent::SettingsChanged(changed) if changed.key == keys::ASR_ENGINE
        )));
        assert!(matches!(
            block_on(factory::run(
                &harness.ctx,
                &SHARED,
                EngineInput {
                    engine_id: SILERO_VAD
                },
                set_active
            )),
            Err(AppError::Validation { .. })
        ));
    }
}
