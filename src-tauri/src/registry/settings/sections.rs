/*!
 * SOURCE OF TRUTH KEYWORDS: SECTIONS, settings sections, section headings, section order, Settings page cards, section page
 * WHAT:  SECTIONS: every settings section with its heading and the page that shows it, in page order.
 * WHY:   The Settings page draws one card per section (04 §5) and takes the heading and order from here, so the UI
 *        never spells a section name and a new section is one entry. The order matches the order in which sections
 *        first appear in SETTINGS (a test proves it), so the list of settings stays the one place that orders them.
 *        The dictionary section lives on its own sidebar page (NavId::Dictionary), every other one on Settings.
 * WHERE: Re-exported by registry/settings; sent to the UI by `registry_get` (RegistryView.sections).
 */

use crate::types::{NavId, SettingSection, SettingSectionSpec, StaticStr};

/// A section shown on the Settings page.
const fn section(section: SettingSection, label: &'static str) -> SettingSectionSpec {
    on_page(section, label, NavId::Settings)
}

/// A section shown on `page`.
const fn on_page(section: SettingSection, label: &'static str, page: NavId) -> SettingSectionSpec {
    SettingSectionSpec {
        section,
        label: StaticStr::new(label),
        page,
    }
}

/// Every Settings page section, in page order.
pub const SECTIONS: &[SettingSectionSpec] = &[
    section(SettingSection::General, "General"),
    section(SettingSection::Pill, "Recording pill"),
    section(SettingSection::Hotkeys, "Hotkeys"),
    section(SettingSection::Session, "Recording"),
    section(SettingSection::Audio, "Microphone"),
    section(SettingSection::Output, "Output"),
    section(SettingSection::Transcription, "Transcription"),
    section(SettingSection::Polish, "Cleanup"),
    on_page(SettingSection::Dictionary, "Dictionary", NavId::Dictionary),
    section(SettingSection::Storage, "Storage"),
    section(SettingSection::Metrics, "Dashboard"),
    section(SettingSection::Privacy, "Privacy"),
    section(SettingSection::Updates, "Updates"),
];
