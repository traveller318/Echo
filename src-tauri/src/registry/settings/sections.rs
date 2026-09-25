/*!
 * SOURCE OF TRUTH KEYWORDS: SECTIONS, settings sections, section headings, section order, Settings page cards
 * WHAT:  SECTIONS: every Settings page section with its heading, in page order.
 * WHY:   The Settings page draws one card per section (04 §5) and takes the heading and order from here, so the UI
 *        never spells a section name and a new section is one entry. The order matches the order in which sections
 *        first appear in SETTINGS (a test proves it), so the list of settings stays the one place that orders them.
 * WHERE: Re-exported by registry/settings; sent to the UI by `registry_get` (RegistryView.sections).
 */

use crate::types::{SettingSection, SettingSectionSpec, StaticStr};

const fn section(section: SettingSection, label: &'static str) -> SettingSectionSpec {
    SettingSectionSpec {
        section,
        label: StaticStr::new(label),
    }
}

/// Every Settings page section, in page order.
pub const SECTIONS: &[SettingSectionSpec] = &[
    section(SettingSection::General, "General"),
    section(SettingSection::Hotkeys, "Hotkeys"),
    section(SettingSection::Session, "Recording"),
    section(SettingSection::Audio, "Microphone"),
    section(SettingSection::Output, "Output"),
    section(SettingSection::Transcription, "Transcription"),
    section(SettingSection::Polish, "Cleanup"),
    section(SettingSection::Storage, "Storage"),
    section(SettingSection::Metrics, "Dashboard"),
    section(SettingSection::Privacy, "Privacy"),
    section(SettingSection::Updates, "Updates"),
];
