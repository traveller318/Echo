/*!
 * SOURCE OF TRUTH KEYWORDS: Permission, PermissionState, Microphone, Network, Clipboard, InputInjection, permission preflight, offline mode
 * WHAT:  The permissions a command or onboarding step can require (Permission) and the answer a check gives
 *        (PermissionState).
 * WHY:   The identity of a permission is a type (it travels inside AppError::PermissionDenied and command specs);
 *        how each one is checked is behaviour, so the `check` fn lives in the registry permissions entry (02 §3.3).
 *        `ALL` lets the registry prove every permission has exactly one entry.
 * WHERE: Declared by commands through the factory, checked via registry/permissions, answered for the microphone
 *        by the PrivacyConsent port, shown by onboarding and src/lib/app-error.ts.
 */

use serde::{Deserialize, Serialize};
use specta::Type;

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
}
