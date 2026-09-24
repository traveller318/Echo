/*!
 * SOURCE OF TRUTH KEYWORDS: SettingsPage, system settings page, microphone privacy page, SystemLauncher input, open settings
 * WHAT:  SettingsPage: the operating-system settings pages Echo can open for the user (so far the microphone
 *        privacy page).
 * WHY:   The SystemLauncher port takes a page by meaning, not by URI: `ms-settings:privacy-microphone` is a Windows
 *        detail that belongs to the adapter (root CLAUDE.md §3), so a second launcher (or a Windows release that
 *        renames the page) changes one adapter, never a command. A closed enum keeps the adapter's mapping
 *        exhaustive, so a new page fails to compile until it has a target.
 * WHERE: Passed by ipc/commands/system.rs (`app_open_mic_privacy_settings`) to ports::SystemLauncher; mapped to a
 *        URI in adapters/launcher.
 */

/// An operating-system settings page Echo can open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SettingsPage {
    /// Where the user allows desktop apps to use the microphone (05 W13).
    MicrophonePrivacy,
}
