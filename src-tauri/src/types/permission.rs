/*!
 * SOURCE OF TRUTH KEYWORDS: Permission, PermissionState, PermissionGate, Microphone, Network, Clipboard, InputInjection, permission preflight, offline mode
 * WHAT:  The permissions a command or onboarding step can require (Permission), the answer a check gives
 *        (PermissionState) and PermissionGate, a check an adapter can repeat while it works.
 * WHY:   The identity of a permission is a type (it travels inside AppError::PermissionDenied and command specs);
 *        how each one is checked is behaviour, so the `check` fn lives in the registry permissions entry (02 §3.3).
 *        `ALL` lets the registry prove every permission has exactly one entry.
 * WHERE: Declared by commands through the factory, checked via registry/permissions, answered for the microphone
 *        by the PrivacyConsent port, shown by onboarding and src/lib/app-error.ts.
 */

use std::{fmt, sync::Arc};

use serde::{Deserialize, Serialize};
use specta::Type;

use super::{AppError, PortResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    /// Windows microphone privacy consent for desktop apps.
    Microphone,
    /// Any outbound request; offline mode denies it.
    Network,
    /// Reading or writing the system clipboard.
    Clipboard,
    /// Sending synthetic input (the Ctrl+V paste) to the focused app.
    InputInjection,
}

impl Permission {
    /// Every permission, in declaration order.
    pub const ALL: [Self; 4] = [
        Self::Microphone,
        Self::Network,
        Self::Clipboard,
        Self::InputInjection,
    ];
}

/// Whether a permission holds right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum PermissionState {
    Granted,
    Denied,
}

impl PermissionState {
    pub const fn is_granted(self) -> bool {
        matches!(self, Self::Granted)
    }
}

/// How a PermissionGate answers.
pub type PermissionCheckFn = dyn Fn() -> PortResult<PermissionState> + Send + Sync;

/**
 * SOURCE OF TRUTH KEYWORDS: PermissionGate, require permission, offline gate, network gate, repeated permission check
 * WHAT:  A permission check an adapter can run again at any moment: `require()` is Ok while the permission holds and
 *        the permission's denial error (from its registry entry, e.g. `Offline` for Network) once it does not.
 * WHY:   The command factory checks a permission once, before the handler (02 §4.1); a model download runs for
 *        minutes, and offline mode switched on halfway must stop it at once (02 §10: "rejects everything" while
 *        offline). The gate is built in app/ from the registry's own check over the live settings, so the adapter
 *        learns nothing about settings keys and the answer is the one the factory gives. A check that cannot be
 *        answered returns its error, never a guess.
 * WHERE: Built by registry/permissions.rs (`gate`) in app/bootstrap; held by adapters/net (HttpClient) and asked
 *        before every request and every received chunk.
 */
#[derive(Clone)]
pub struct PermissionGate {
    permission: Permission,
    /// Behind an Arc so the gate stays two pointers wide inside the HTTP body it travels with.
    denied: Arc<AppError>,
    check: Arc<PermissionCheckFn>,
}

impl PermissionGate {
    pub fn new(permission: Permission, denied: AppError, check: Arc<PermissionCheckFn>) -> Self {
        Self {
            permission,
            denied: Arc::new(denied),
            check,
        }
    }

    /// The permission this gate guards.
    pub const fn permission(&self) -> Permission {
        self.permission
    }

    /// Ok while the permission holds; the gate's denial error when it does not.
    pub fn require(&self) -> PortResult<()> {
        if (self.check)()?.is_granted() {
            Ok(())
        } else {
            Err(AppError::clone(&self.denied).into())
        }
    }
}

impl fmt::Debug for PermissionGate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PermissionGate")
            .field("permission", &self.permission)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permissions_and_states_serialize_in_snake_case() {
        assert_eq!(
            serde_json::to_value(Permission::InputInjection).unwrap(),
            "input_injection"
        );
        assert_eq!(
            serde_json::to_value(PermissionState::Denied).unwrap(),
            "denied"
        );
        assert!(PermissionState::Granted.is_granted());
        assert!(!PermissionState::Denied.is_granted());
    }

    #[test]
    fn a_gate_answers_at_each_call() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let online = Arc::new(AtomicBool::new(true));
        let reading = Arc::clone(&online);
        let gate = PermissionGate::new(
            Permission::Network,
            AppError::Offline,
            Arc::new(move || {
                Ok(if reading.load(Ordering::SeqCst) {
                    PermissionState::Granted
                } else {
                    PermissionState::Denied
                })
            }),
        );
        assert!(gate.require().is_ok());
        online.store(false, Ordering::SeqCst);
        assert_eq!(
            gate.require()
                .map_err(super::super::PortError::into_app_error),
            Err(AppError::Offline)
        );
        assert_eq!(gate.permission(), Permission::Network);
    }
}
