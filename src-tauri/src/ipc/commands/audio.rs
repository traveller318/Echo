/*!
 * SOURCE OF TRUTH KEYWORDS: audio commands, audio_list_devices, audio_test_level, microphone check, input devices, MicCheck
 * WHAT:  The audio command group (02 §4.3): `audio_list_devices` returns the input devices Windows has now;
 *        `audio_test_level` listens to one for a short window, sends its live AudioLevel events meanwhile (the
 *        meter a caller draws) and returns its levels and a verdict.
 * WHY:   The device list feeds the `audio.input_device` setting (kind Device) and onboarding; it is read on demand,
 *        never cached, because devices come and go. The microphone check declares the Microphone permission, so a
 *        blocked Windows privacy consent is refused by the factory before the device opens (05 W13), and it is
 *        Exclusive: two checks never fight over the device (a take in progress makes the port answer `Busy`). The
 *        handler only picks the device and window; capture, conversion and the verdict live in pipeline/capture.
 * WHERE: Registered through `ipc::commands::catalog`; called from the UI as `commands.audioListDevices()` and
 *        `commands.audioTestLevel({ device, window_ms })` by Settings (step 18) and onboarding (step 24).
 */

use std::time::Duration;

use crate::{
    ipc::{CommandCtx, factory::echo_command},
    pipeline::capture,
    types::{AudioDevice, AudioTestLevelInput, MicCheck, Permission, PortError},
};

echo_command! {
    /// The audio input devices Windows has right now, with the current default marked.
    name: audio_list_devices,
    output: Vec<AudioDevice>,
    permission: None,
    reentrancy: Shared,
    handler: list_devices,
}

echo_command! {
    /// Listens to an input device (none = the Windows default) for `window_ms` and reports how it sounded.
    name: audio_test_level,
    input: AudioTestLevelInput,
    output: MicCheck,
    permission: Some(Permission::Microphone),
    reentrancy: Exclusive("microphone"),
    handler: test_level,
}

/// Lists the input devices.
pub async fn list_devices(ctx: &CommandCtx, (): ()) -> Result<Vec<AudioDevice>, PortError> {
    ctx.audio().devices()
}

/// Runs the microphone check on the requested device.
pub async fn test_level(
    ctx: &CommandCtx,
    input: AudioTestLevelInput,
) -> Result<MicCheck, PortError> {
    capture::check_microphone(
        ctx.audio(),
        ctx.scheduler(),
        input.device.as_ref(),
        Duration::from_millis(u64::from(input.window_ms)),
        Some(ctx.event_sink()),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ipc::{factory, testing},
        ports::fakes::FakePrivacyConsent,
        registry,
        types::{
            AppError, AudioDeviceId, CommandSpec, MicVerdict, PermissionState, Reentrancy,
            ResourceKind,
        },
    };

    const LIST: CommandSpec = CommandSpec {
        name: "audio_list_devices",
        permission: None,
        reentrancy: Reentrancy::Shared,
    };

    const TEST_LEVEL: CommandSpec = CommandSpec {
        name: "audio_test_level",
        permission: Some(Permission::Microphone),
        reentrancy: Reentrancy::Exclusive("microphone"),
    };

    fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(future)
    }

    fn input(device: Option<&str>) -> AudioTestLevelInput {
        AudioTestLevelInput {
            device: device.map(|id| AudioDeviceId::from(id.to_owned())),
            window_ms: AudioTestLevelInput::MIN_WINDOW_MS,
        }
    }

    #[test]
    fn lists_the_devices_the_port_reports() {
        let harness = testing::harness(
            registry::settings::defaults(),
            FakePrivacyConsent::granted(),
        );
        let devices = block_on(factory::run(&harness.ctx, &LIST, (), list_devices)).unwrap();
        assert_eq!(devices, harness.ctx.audio().devices().unwrap());
    }

    #[test]
    fn a_speaking_microphone_passes_the_check_and_is_closed_after() {
        let harness = testing::harness(
            registry::settings::defaults(),
            FakePrivacyConsent::granted(),
        );
        harness.audio.play_on_start(vec![0.2; 48_000]);
        let check = block_on(factory::run(
            &harness.ctx,
            &TEST_LEVEL,
            input(None),
            test_level,
        ))
        .unwrap();
        assert_eq!(check.verdict, MicVerdict::Good);
        assert!(!harness.audio.is_open());
        assert_eq!(harness.scheduler.requests().len(), 1);
    }

    #[test]
    fn a_blocked_microphone_is_refused_before_the_device_opens() {
        let harness = testing::harness(
            registry::settings::defaults(),
            FakePrivacyConsent::new(PermissionState::Denied),
        );
        let result = block_on(factory::run(
            &harness.ctx,
            &TEST_LEVEL,
            input(None),
            test_level,
        ));
        assert_eq!(
            result,
            Err(AppError::PermissionDenied {
                permission: Permission::Microphone
            })
        );
        assert_eq!(harness.audio.starts(), 0);
    }

    #[test]
    fn a_missing_device_and_a_bad_window_are_reported() {
        let harness = testing::harness(
            registry::settings::defaults(),
            FakePrivacyConsent::granted(),
        );
        let missing = block_on(factory::run(
            &harness.ctx,
            &TEST_LEVEL,
            input(Some("unplugged")),
            test_level,
        ));
        assert_eq!(
            missing,
            Err(AppError::NotFound {
                resource: ResourceKind::AudioDevice
            })
        );
        let too_long = AudioTestLevelInput {
            window_ms: AudioTestLevelInput::MAX_WINDOW_MS + 1,
            ..input(None)
        };
        let invalid = block_on(factory::run(
            &harness.ctx,
            &TEST_LEVEL,
            too_long,
            test_level,
        ));
        assert!(
            matches!(invalid, Err(AppError::Validation { ref field, .. }) if field == "window_ms")
        );
    }
}
