/*!
 * SOURCE OF TRUTH KEYWORDS: LinuxPrivacyConsent, microphone consent Linux, no privacy switch
 * WHAT:  LinuxPrivacyConsent: PrivacyConsent that always reports the microphone as granted.
 * WHY:   A desktop Linux session has no per-app microphone switch for native apps (only sandboxed Flatpak/Snap apps
 *        ask through a portal, and Echo is installed natively); a sound server that refuses access still makes the
 *        open fail, which the capture adapter maps to `PermissionDenied { microphone }`.
 * WHERE: Built by app/bootstrap on Linux; the Windows build reads the privacy registry (win32.rs).
 */

use crate::{
    ports::PrivacyConsent,
    types::{PermissionState, PortResult},
};

#[derive(Debug, Default)]
pub struct LinuxPrivacyConsent;

impl LinuxPrivacyConsent {
    pub const fn new() -> Self {
        Self
    }
}

impl PrivacyConsent for LinuxPrivacyConsent {
    fn microphone(&self) -> PortResult<PermissionState> {
        Ok(PermissionState::Granted)
    }
}
