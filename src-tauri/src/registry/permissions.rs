/*!
 * SOURCE OF TRUTH KEYWORDS: permission registry, PERMISSIONS, PermissionCtx, permission check, permission gate, offline mode denies network, microphone consent, preflight
 * WHAT:  One entry per Permission with the fn that decides whether it holds right now, the context those fns
 *        read (PermissionCtx: resolved settings and the privacy consent port), `check` to run one and `gate` to
 *        hand the same check to an adapter that must ask again while it works.
 * WHY:   The command factory's preflight (02 §4.1 step 3) and onboarding ask the registry instead of each
 *        command re-checking (root CLAUDE.md §3). Network is denied unless offline mode is explicitly off, so a
 *        missing value fails closed (privacy first, 02 §10). The microphone asks Windows privacy consent through
 *        a port (05 W13). Clipboard and input injection have no OS consent gate on Windows desktop: a busy
 *        clipboard (05 W4) and an elevated paste target (05 W2) are runtime outcomes handled by delivery, not
 *        permissions, so their checks grant. A failed consent read is returned, never guessed; the caller logs
 *        the detail and decides. PermissionCtx lives here because it holds a port handle (types/ cannot).
 * WHERE: `check` is called by ipc/factory.rs (preflight), onboarding and pipeline/models (`models_list`'s network
 *        state); `gate` by app/bootstrap for the HTTP client (adapters/net); tests below.
 */

use std::sync::Arc;

use super::settings::keys;
use crate::{
    ports::PrivacyConsent,
    types::{
        AppError, Permission, PermissionGate, PermissionState, PortError, PortResult,
        SettingsSnapshot, SharedSettings,
    },
};

/// What a permission check may read.
pub struct PermissionCtx<'a> {
    pub settings: &'a SettingsSnapshot,
    pub consent: &'a dyn PrivacyConsent,
}

pub type PermissionCheck = fn(&PermissionCtx<'_>) -> PortResult<PermissionState>;

/// A registry permission entry.
pub struct PermissionEntry {
    pub permission: Permission,
    pub check: PermissionCheck,
}

/// Every permission with its check.
pub const PERMISSIONS: &[PermissionEntry] = &[
    PermissionEntry {
        permission: Permission::Microphone,
        check: microphone,
    },
    PermissionEntry {
        permission: Permission::Network,
        check: network,
    },
    PermissionEntry {
        permission: Permission::Clipboard,
        check: always_granted,
    },
    PermissionEntry {
        permission: Permission::InputInjection,
        check: always_granted,
    },
];

/// Whether `permission` holds right now.
pub fn check(permission: Permission, ctx: &PermissionCtx<'_>) -> PortResult<PermissionState> {
    let entry = PERMISSIONS
        .iter()
        .find(|entry| entry.permission == permission)
        .ok_or_else(|| {
            PortError::new(AppError::Internal)
                .with_detail(format!("no registry entry for permission {permission:?}"))
        })?;
    (entry.check)(ctx)
}

/**
 * SOURCE OF TRUTH KEYWORDS: permission gate, live permission check, offline mode mid-download, gate builder
 * WHAT:  A PermissionGate that runs `permission`'s registry check over the settings in effect at each call.
 * WHY:   The factory checks once before a command; a long transfer must stop the moment offline mode is switched
 *        on, with the same answer the factory would give, without the adapter knowing any setting.
 * WHERE: app/bootstrap (the HTTP client's network gate).
 */
pub fn gate(
    permission: Permission,
    settings: SharedSettings,
    consent: Arc<dyn PrivacyConsent>,
) -> PermissionGate {
    PermissionGate::new(
        permission,
        Arc::new(move || {
            check(
                permission,
                &PermissionCtx {
                    settings: &settings.current(),
                    consent: consent.as_ref(),
                },
            )
        }),
    )
}

fn microphone(ctx: &PermissionCtx<'_>) -> PortResult<PermissionState> {
    ctx.consent.microphone()
}

fn network(ctx: &PermissionCtx<'_>) -> PortResult<PermissionState> {
    Ok(match ctx.settings.bool(&keys::OFFLINE_MODE) {
        Some(false) => PermissionState::Granted,
        Some(true) | None => PermissionState::Denied,
    })
}

fn always_granted(_: &PermissionCtx<'_>) -> PortResult<PermissionState> {
    Ok(PermissionState::Granted)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::{
        ports::fakes::FakePrivacyConsent,
        registry::settings,
        types::{SettingKey, SettingValue},
    };

    fn state(
        permission: Permission,
        settings: &SettingsSnapshot,
        consent: &FakePrivacyConsent,
    ) -> PermissionState {
        check(permission, &PermissionCtx { settings, consent }).unwrap()
    }

    #[test]
    fn every_permission_has_exactly_one_entry() {
        let registered: HashSet<Permission> =
            PERMISSIONS.iter().map(|entry| entry.permission).collect();
        assert_eq!(registered.len(), PERMISSIONS.len());
        assert_eq!(registered, HashSet::from(Permission::ALL));
    }

    #[test]
    fn offline_mode_denies_network() {
        let consent = FakePrivacyConsent::granted();
        let online = settings::defaults();
        let offline = settings::resolve([(settings::keys::OFFLINE_MODE, SettingValue::Bool(true))]);
        assert_eq!(
            state(Permission::Network, &online, &consent),
            PermissionState::Granted
        );
        assert_eq!(
            state(Permission::Network, &offline, &consent),
            PermissionState::Denied
        );
    }

    #[test]
    fn network_fails_closed_without_a_resolved_offline_setting() {
        let consent = FakePrivacyConsent::granted();
        let unresolved = SettingsSnapshot::from_resolved([(
            SettingKey::from_static("privacy.offline_mode"),
            SettingValue::Int(0),
        )]);
        assert_eq!(
            state(Permission::Network, &unresolved, &consent),
            PermissionState::Denied
        );
        assert_eq!(
            state(Permission::Network, &SettingsSnapshot::default(), &consent),
            PermissionState::Denied
        );
    }

    #[test]
    fn microphone_follows_windows_privacy_consent() {
        let consent = FakePrivacyConsent::new(PermissionState::Denied);
        let defaults = settings::defaults();
        assert_eq!(
            state(Permission::Microphone, &defaults, &consent),
            PermissionState::Denied
        );
        consent.set_microphone(PermissionState::Granted);
        assert_eq!(
            state(Permission::Microphone, &defaults, &consent),
            PermissionState::Granted
        );
        consent.fail_next(
            PortError::new(AppError::AudioDevice).with_detail("consent store unreadable"),
        );
        let failed = check(
            Permission::Microphone,
            &PermissionCtx {
                settings: &defaults,
                consent: &consent,
            },
        );
        assert_eq!(
            failed.map_err(|error| error.into_app_error()),
            Err(AppError::AudioDevice)
        );
    }

    #[test]
    fn a_gate_follows_the_live_settings() {
        let live = SharedSettings::new(settings::defaults());
        let network = gate(
            Permission::Network,
            live.clone(),
            Arc::new(FakePrivacyConsent::granted()),
        );
        assert!(network.require().is_ok());
        live.replace(settings::resolve([(
            settings::keys::OFFLINE_MODE,
            SettingValue::Bool(true),
        )]));
        assert_eq!(
            network.require().map_err(PortError::into_app_error),
            Err(AppError::PermissionDenied {
                permission: Permission::Network
            })
        );
    }

    #[test]
    fn clipboard_and_input_injection_have_no_consent_gate() {
        let consent = FakePrivacyConsent::new(PermissionState::Denied);
        let offline = settings::resolve([(settings::keys::OFFLINE_MODE, SettingValue::Bool(true))]);
        assert_eq!(
            state(Permission::Clipboard, &offline, &consent),
            PermissionState::Granted
        );
        assert_eq!(
            state(Permission::InputInjection, &offline, &consent),
            PermissionState::Granted
        );
    }
}
