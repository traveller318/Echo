/*!
 * SOURCE OF TRUTH KEYWORDS: settings registry, SETTINGS, setting keys, setting defaults, resolve settings, runtime options, validate setting, SettingsSnapshot
 * WHAT:  Every setting Echo has (02 §3.3) with its section, label, help, kind and bounds, default,
 *        restart_required and visibility (list.rs); typed key constants (keys.rs) and compared enum values
 *        (values.rs); and the operations on them: resolve stored values into a SettingsSnapshot (resolve.rs), typed
 *        reads of a snapshot (reads.rs), resolve an OptionSource into options and validate a write (options.rs).
 * WHY:   One list drives the settings service, the generated Settings UI and its Zod schema, so adding a setting
 *        is one entry here (root CLAUDE.md §7). Options that depend on what is installed (engines, their
 *        languages and accelerators) are OptionSources resolved from registry/engines at runtime, never a
 *        hardcoded list. Callers use the `keys` constants, never string literals, so a renamed key fails to
 *        compile instead of silently reading a default. Hotkey defaults come from registry/hotkeys so a hotkey
 *        and its setting cannot disagree. Split by responsibility once the single file passed 500 lines; every
 *        public item is re-exported here, so callers keep the `registry::settings::…` paths.
 * WHERE: Read by services/settings callers (commands validate writes with `validate`), the pipeline (via
 *        `resolve` and the typed reads), registry/permissions (offline mode), registry/hotkeys and `registry_get`.
 */

pub mod keys;
mod list;
mod options;
mod reads;
mod resolve;
pub mod values;

#[cfg(test)]
mod tests;

pub use list::SETTINGS;
pub use options::{options, validate};
pub use reads::{
    accelerator_preference, asr_engine, delivery_policy, dictionary, language_preference,
    llm_polisher, record_mode, remove_fillers, session_policy, theme, trailing_space,
};
pub use resolve::{defaults, find, resolve};
