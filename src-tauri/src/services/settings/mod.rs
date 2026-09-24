/*!
 * SOURCE OF TRUTH KEYWORDS: settings service, settings table, stored settings, value_json, get set reset settings
 * WHAT:  The `settings` table's verbs, one file each: get (every stored row, or one), set (upsert one value) and
 *        reset (remove one stored value so its registry default applies again).
 * WHY:   The table holds only values the user changed; defaults live in the registry and are overlaid by
 *        `registry::settings::resolve` (02 §7.2). Values are stored as the SettingValue JSON (kind-tagged), so a
 *        row names its own kind. Validation against the registry happens in the command layer before `set`
 *        (02 §7.2); this service stays pure storage.
 * WHERE: `use crate::services::settings` from app/bootstrap (initial resolve) and ipc/commands/settings.rs.
 */

pub mod get;
pub mod reset;
pub mod set;
