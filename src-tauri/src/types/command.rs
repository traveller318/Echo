/*!
 * SOURCE OF TRUTH KEYWORDS: CommandSpec, Reentrancy, Shared, Exclusive, reentrancy key, command declaration, permission requirement, echo_command
 * WHAT:  What an IPC command declares to the command factory: its name, the permission it needs (if any) and its
 *        reentrancy rule (CommandSpec / Reentrancy).
 * WHY:   The factory runs the same pipeline for every command (02 §4.1) and reads everything command-specific from
 *        this one `const`, so a handler never re-checks a permission or guards itself. The declaration is plain
 *        data (no handler, no Tauri type) so it lives in types/ and tests can build one without an app. An
 *        `Exclusive` key is a `&'static str` shared by every command that must not overlap (e.g. all model
 *        transfers use `model_transfer`); a second caller gets `AppError::Busy` instead of queueing behind the
 *        first. Nothing here crosses IPC.
 * WHERE: Built by the `echo_command!` macro (ipc/factory.rs) from each declaration in ipc/commands; read by
 *        `factory::run` (tracing span name, permission preflight, reentrancy guard, outcome metric).
 */

use super::Permission;

/// Whether a command may overlap with itself or with commands sharing its key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Reentrancy {
    /// Any number of calls may run at once (reads, idempotent writes).
    Shared,
    /// At most one call holding this key runs at a time; another caller gets `AppError::Busy`.
    Exclusive(&'static str),
}

/// A command's declaration, as the factory sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CommandSpec {
    /// The command name, `<group>_<verb>` (03 §3); also its tracing span and metric label.
    pub name: &'static str,
    /// Checked through the registry before the handler runs; None when the command needs none.
    pub permission: Option<Permission>,
    pub reentrancy: Reentrancy,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specs_are_const_and_compare_by_value() {
        const DOWNLOAD: CommandSpec = CommandSpec {
            name: "models_download",
            permission: Some(Permission::Network),
            reentrancy: Reentrancy::Exclusive("model_transfer"),
        };
        assert_eq!(DOWNLOAD.reentrancy, Reentrancy::Exclusive("model_transfer"));
        assert_ne!(DOWNLOAD.reentrancy, Reentrancy::Shared);
        assert_eq!(DOWNLOAD.permission, Some(Permission::Network));
    }
}
