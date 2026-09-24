/*!
 * SOURCE OF TRUTH KEYWORDS: PrivacyConsent, microphone consent, Windows privacy settings, CapabilityAccessManager, mic blocked, permission check
 * WHAT:  PrivacyConsent reports whether the operating system's privacy settings let Echo use the microphone.
 * WHY:   With Windows mic privacy off, cpal still opens the device but delivers silence or an error (05 W13), so
 *        the consent is read before a take instead of guessed from audio. Reading it is a Windows API call (the
 *        CapabilityAccessManager consent store), which belongs behind a port (root CLAUDE.md §3), and the
 *        registry `Microphone` permission check needs something to ask. A failed read is reported, not guessed,
 *        so the caller decides and logs the detail. Cheap and blocking: one registry read.
 * WHERE: Implemented by adapters/consent (Win32PrivacyConsent) and ports/fakes; asked by registry::permissions through
 *        PermissionCtx for the factory preflight and onboarding.
 */

use crate::types::{PermissionState, PortResult};

/// Operating-system privacy consent.
pub trait PrivacyConsent: Send + Sync {
    /// `Denied` when Windows blocks microphone access for desktop apps.
    fn microphone(&self) -> PortResult<PermissionState>;
}
