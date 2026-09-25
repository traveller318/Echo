/*!
 * SOURCE OF TRUTH KEYWORDS: registry layer, single source of truth, settings registry, engines registry, models, nav, network allowlist, hotkeys, metrics, permissions, events, sounds, notices
 * WHAT:  Layer 4: declarative entries for everything the app has (engines, models, settings, hotkeys, nav,
 *        metrics, permissions, events, sound cues, one-time notices), exported through specta so the UI renders from them. Each module holds
 *        one `const` list plus lookups; entries that need behaviour (engine builders, permission checks) carry a
 *        plain fn pointer.
 * WHY:   Adding a feature is an entry here, not a new pattern; a `match` on a feature name anywhere else is a
 *        defect (02 §3.3). Entries build the selected adapters lazily. Shapes that cross IPC live in types/;
 *        the only shapes declared here (BuildCtx, EngineEntry, PermissionCtx, PermissionEntry) hold ports or fn
 *        pointers, which types/ may not reference (02 §3.2).
 * WHERE: Read by app/, pipeline/ and ipc/; may import types/, ports/ and adapters/.
 */

pub mod engines;
pub mod events;
pub mod hotkeys;
pub mod metrics;
pub mod models;
pub mod nav;
pub mod network;
pub mod notices;
pub mod permissions;
pub mod settings;
pub mod sounds;

/**
 * SOURCE OF TRUTH KEYWORDS: registry id format test, kebab-case id check
 * WHAT:  `is_registry_id` (types/ids.rs, also the command input rule): the shared rule every registry id must follow
 *        in tests.
 * WHY:   Registry ids are kebab-case strings (03 §3), and model versions need dots (`parakeet-tdt-0.6b-v3`);
 *        one checker keeps every registry test and every command input applying the same rule.
 * WHERE: Tests in registry/{engines, models, hotkeys, nav, metrics}.
 */
#[cfg(test)]
mod tests {
    pub(super) use crate::types::is_registry_id;

    #[test]
    fn registry_ids_are_kebab_case() {
        for valid in ["record", "paste-last", "parakeet-tdt-0.6b-v3", "qwen3-1.7b"] {
            assert!(is_registry_id(valid), "{valid}");
        }
        for invalid in [
            "",
            "Record",
            "paste_last",
            "-x",
            "x-",
            "a--b",
            "1st",
            "a b",
            "a..b",
        ] {
            assert!(!is_registry_id(invalid), "{invalid}");
        }
    }
}
