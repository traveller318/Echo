/*!
 * SOURCE OF TRUTH KEYWORDS: consent adapters, PrivacyConsent implementations, Win32PrivacyConsent, microphone privacy
 * WHAT:  Adapters behind the PrivacyConsent port.
 * WHY:   Operating-system privacy consent is a Windows API concern and stays behind its port (root CLAUDE.md §3).
 * WHERE: Constructed by app/bootstrap; used only through `dyn PrivacyConsent`.
 */

mod win32;

pub use win32::Win32PrivacyConsent;
