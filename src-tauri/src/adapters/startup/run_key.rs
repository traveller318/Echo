/*!
 * SOURCE OF TRUTH KEYWORDS: Win32RunKey, HKCU Run, StartupApproved Run, Task Manager startup switch, launch at sign-in entry, quoted executable path, RunKeyLocation
 * WHAT:  Win32RunKey: LaunchAtLogin over the user's `Run` key. The entry is one REG_SZ value (named after the app
 *        identifier) holding the quoted executable path and Echo's launch arguments; `state` also reads the
 *        matching `StartupApproved\Run` value, where Task Manager's Startup apps page records a switch-off.
 * WHY:   Per-user (HKCU) needs no admin rights, like the per-user installer (02 §11). The path is quoted: an
 *        unquoted path with spaces (a user name with a space) would make Windows run the wrong file or nothing.
 *        Task Manager does not delete the Run value when the user switches Echo off, it writes a StartupApproved
 *        value whose first byte has the low bit set (0x03, with the switch-off time after it; 0x02 or 0x06 means
 *        on). Reading it lets the core respect that choice (`approved: false`); `register` writes "on" there only
 *        when it said off, because it runs only for the user's own choice in Echo. The tauri-plugin-autostart crate
 *        was not used: it writes the path unquoted and re-enables a Task Manager switch-off on every call (05
 *        decision log, step 25). Key locations are a RunKeyLocation so tests use a scratch key, never the real Run
 *        key.
 * WHERE: Built by app/bootstrap (installed builds only) and held by CommandCtx; driven by pipeline/launch.rs.
 */

use std::path::Path;

use windows::Win32::System::Registry::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_SAM_FLAGS};

use crate::{
    adapters::win32::RegistryKey,
    ports::LaunchAtLogin,
    types::{LaunchAtLoginCaps, LaunchAtLoginState, PortResult},
};

/// Where Windows keeps the start-at-sign-in entries, under HKEY_CURRENT_USER.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunKeyLocation {
    /// The key holding the command lines.
    pub run: String,
    /// The key where Task Manager records its switch.
    pub approved: String,
}

impl RunKeyLocation {
    /// The signed-in user's keys.
    pub fn current_user() -> Self {
        Self {
            run: String::from(r"Software\Microsoft\Windows\CurrentVersion\Run"),
            approved: String::from(
                r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run",
            ),
        }
    }
}

/// The StartupApproved value Task Manager writes for an entry that is switched on.
const APPROVED_ON: [u8; 12] = [0x02, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

/// Echo's entry in the Run key.
pub struct Win32RunKey {
    location: RunKeyLocation,
    value_name: String,
    command: String,
}

impl Win32RunKey {
    /// An entry named `value_name` that starts `executable` with `args` (e.g. `--minimized`).
    pub fn new(
        location: RunKeyLocation,
        value_name: &str,
        executable: &Path,
        args: &[&str],
    ) -> Self {
        Self {
            location,
            value_name: value_name.to_owned(),
            command: command_line(executable, args),
        }
    }

    fn run_key(&self, access: REG_SAM_FLAGS) -> PortResult<Option<RegistryKey>> {
        RegistryKey::open(HKEY_CURRENT_USER, &self.location.run, access)
    }

    fn approved_key(&self, access: REG_SAM_FLAGS) -> PortResult<Option<RegistryKey>> {
        RegistryKey::open(HKEY_CURRENT_USER, &self.location.approved, access)
    }
}

impl LaunchAtLogin for Win32RunKey {
    fn caps(&self) -> LaunchAtLoginCaps {
        LaunchAtLoginCaps { available: true }
    }

    fn state(&self) -> PortResult<LaunchAtLoginState> {
        let registered = self
            .run_key(KEY_READ)?
            .map(|key| key.read_string(&self.value_name))
            .transpose()?
            .flatten();
        let Some(registered) = registered else {
            return Ok(LaunchAtLoginState::NotRegistered);
        };
        let approval = self
            .approved_key(KEY_READ)?
            .map(|key| key.read_binary(&self.value_name))
            .transpose()?
            .flatten();
        Ok(LaunchAtLoginState::Registered {
            approved: approval.as_deref().is_none_or(approved),
            current: same_command(&registered, &self.command),
        })
    }

    fn register(&self) -> PortResult<()> {
        RegistryKey::create(HKEY_CURRENT_USER, &self.location.run, KEY_WRITE)?
            .write_string(&self.value_name, &self.command)?;
        // Task Manager's switch-off would keep the new entry from running; the user just asked for it in Echo.
        if let Some(key) = self.approved_key(KEY_READ | KEY_WRITE)?
            && key
                .read_binary(&self.value_name)?
                .is_some_and(|value| !approved(&value))
        {
            key.write_binary(&self.value_name, &APPROVED_ON)?;
        }
        Ok(())
    }

    fn unregister(&self) -> PortResult<()> {
        if let Some(key) = self.run_key(KEY_WRITE)? {
            key.delete_value(&self.value_name)?;
        }
        if let Some(key) = self.approved_key(KEY_WRITE)? {
            key.delete_value(&self.value_name)?;
        }
        Ok(())
    }
}

/// The Run value for `executable` with `args`: the path always quoted.
fn command_line(executable: &Path, args: &[&str]) -> String {
    std::iter::once(format!("\"{}\"", executable.display()))
        .chain(args.iter().map(|arg| (*arg).to_owned()))
        .collect::<Vec<_>>()
        .join(" ")
}

/// A StartupApproved value says "on" unless the low bit of its first byte is set; an empty one says nothing (on).
fn approved(value: &[u8]) -> bool {
    value.first().is_none_or(|flags| flags & 1 == 0)
}

/// Windows paths are case-insensitive, so an entry written by another casing of the same path is current.
fn same_command(registered: &str, expected: &str) -> bool {
    registered.trim().eq_ignore_ascii_case(expected.trim())
}

#[cfg(test)]
mod tests {
    use windows::{Win32::System::Registry::RegDeleteTreeW, core::HSTRING};

    use super::*;

    /// A scratch Run / StartupApproved pair under HKCU\Software, deleted on drop.
    struct Scratch {
        root: String,
        location: RunKeyLocation,
    }

    impl Scratch {
        fn new(name: &str) -> Self {
            let root = format!(
                r"Software\app.echo.desktop.tests\{name}-{}",
                std::process::id()
            );
            let location = RunKeyLocation {
                run: format!(r"{root}\Run"),
                approved: format!(r"{root}\StartupApproved\Run"),
            };
            Self { root, location }
        }

        fn entry(&self) -> Win32RunKey {
            Win32RunKey::new(
                self.location.clone(),
                "app.echo.desktop",
                Path::new(r"\\host\Echo apps\echo.exe"),
                &["--minimized"],
            )
        }

        fn approval(&self, value: &[u8]) {
            RegistryKey::create(HKEY_CURRENT_USER, &self.location.approved, KEY_WRITE)
                .unwrap()
                .write_binary("app.echo.desktop", value)
                .unwrap();
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            // SAFETY: deletes only the scratch tree this test created.
            let _ =
                unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, &HSTRING::from(self.root.as_str())) };
        }
    }

    #[test]
    fn the_command_line_quotes_the_path_and_keeps_the_arguments() {
        assert_eq!(
            command_line(Path::new(r"\\host\Echo apps\echo.exe"), &["--minimized"]),
            r#""\\host\Echo apps\echo.exe" --minimized"#
        );
        assert!(same_command(
            r#" "\\HOST\echo apps\ECHO.exe" --minimized "#,
            r#""\\host\Echo apps\echo.exe" --minimized"#
        ));
        assert!(approved(&[0x02, 0]) && approved(&[0x06]) && approved(&[]));
        assert!(!approved(&[0x03, 0, 0, 0, 1]) && !approved(&[0x07]));
    }

    #[test]
    fn register_state_and_unregister_round_trip_on_a_scratch_key() {
        let scratch = Scratch::new("run-key");
        let entry = scratch.entry();
        assert_eq!(entry.state().unwrap(), LaunchAtLoginState::NotRegistered);
        entry.unregister().unwrap();

        entry.register().unwrap();
        assert_eq!(
            entry.state().unwrap(),
            LaunchAtLoginState::Registered {
                approved: true,
                current: true
            }
        );

        // Task Manager switched it off: the entry stays but Windows will not run it.
        scratch.approval(&[0x03, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(
            entry.state().unwrap(),
            LaunchAtLoginState::Registered {
                approved: false,
                current: true
            }
        );

        // An older install location is not current; registering again repairs it and switches it back on.
        let moved = Win32RunKey::new(
            scratch.location.clone(),
            "app.echo.desktop",
            Path::new(r"\\host\Echo apps 2\echo.exe"),
            &["--minimized"],
        );
        assert!(matches!(
            moved.state().unwrap(),
            LaunchAtLoginState::Registered { current: false, .. }
        ));
        moved.register().unwrap();
        assert_eq!(
            moved.state().unwrap(),
            LaunchAtLoginState::Registered {
                approved: true,
                current: true
            }
        );

        moved.unregister().unwrap();
        assert_eq!(moved.state().unwrap(), LaunchAtLoginState::NotRegistered);
    }
}
