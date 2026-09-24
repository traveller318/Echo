/*!
 * SOURCE OF TRUTH KEYWORDS: FakePrivacyConsent, fake microphone consent, mic blocked test, permission check test
 * WHAT:  FakePrivacyConsent: a PrivacyConsent whose microphone answer the test sets, or whose next read fails.
 * WHY:   The Microphone permission check, the factory preflight and onboarding need the granted, blocked and
 *        unreadable paths without touching Windows privacy settings.
 * WHERE: registry::permissions tests; factory and onboarding tests.
 */

use std::sync::Mutex;

use super::lock;
use crate::{
    ports::PrivacyConsent,
    types::{PermissionState, PortError, PortResult},
};

struct ConsentState {
    microphone: PermissionState,
    next_error: Option<PortError>,
}

/// A settable privacy consent.
pub struct FakePrivacyConsent {
    state: Mutex<ConsentState>,
}

impl FakePrivacyConsent {
    pub fn new(microphone: PermissionState) -> Self {
        Self {
            state: Mutex::new(ConsentState {
                microphone,
                next_error: None,
            }),
        }
    }

    pub fn granted() -> Self {
        Self::new(PermissionState::Granted)
    }

    pub fn set_microphone(&self, state: PermissionState) {
        lock(&self.state).microphone = state;
    }

    pub fn fail_next(&self, error: PortError) {
        lock(&self.state).next_error = Some(error);
    }
}

impl PrivacyConsent for FakePrivacyConsent {
    fn microphone(&self) -> PortResult<PermissionState> {
        let mut state = lock(&self.state);
        match state.next_error.take() {
            Some(error) => Err(error),
            None => Ok(state.microphone),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::AppError;

    #[test]
    fn reports_the_set_state_and_a_single_failure() {
        let consent = FakePrivacyConsent::granted();
        assert_eq!(consent.microphone().unwrap(), PermissionState::Granted);
        consent.set_microphone(PermissionState::Denied);
        assert_eq!(consent.microphone().unwrap(), PermissionState::Denied);
        consent.fail_next(AppError::Internal.into());
        assert!(consent.microphone().is_err());
        assert_eq!(consent.microphone().unwrap(), PermissionState::Denied);
    }
}
