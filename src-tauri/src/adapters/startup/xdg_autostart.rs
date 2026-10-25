/*!
 * SOURCE OF TRUTH KEYWORDS: XdgAutostart, ~/.config/autostart, desktop entry, start at sign-in Linux, AppImage path, X-GNOME-Autostart-enabled, Hidden
 * WHAT:  XdgAutostart: LaunchAtLogin over a desktop entry in `$XDG_CONFIG_HOME/autostart/` (default
 *        `~/.config/autostart/`) named after the app identifier, whose Exec line starts this executable with Echo's
 *        launch arguments.
 * WHY:   The XDG Autostart spec is what GNOME, KDE, XFCE and most other desktops run at sign-in, per user and with no
 *        privileges. A desktop's own switch-off is written into the same file as `Hidden=true` or
 *        `X-GNOME-Autostart-enabled=false`, which `state` reports as not approved (like Task Manager's switch on
 *        Windows) and `register` clears by rewriting the file. Inside an AppImage the running executable lives in a
 *        temporary mount, so the entry names the AppImage file itself ($APPIMAGE). Exec arguments are quoted per the
 *        Desktop Entry spec, so a path with spaces survives.
 * WHERE: Built by app/bootstrap on Linux (a development build gets DisabledLaunchAtLogin); driven by pipeline/launch.rs.
 */

use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

use crate::{
    ports::LaunchAtLogin,
    types::{AppError, LaunchAtLoginCaps, LaunchAtLoginState, PortError, PortResult},
};

/// Echo's autostart entry.
pub struct XdgAutostart {
    file: PathBuf,
    exec: String,
}

impl XdgAutostart {
    /// The entry `<identifier>.desktop` in `autostart_dir`, starting `executable` with `args`.
    pub fn new(autostart_dir: &Path, identifier: &str, executable: &Path, args: &[&str]) -> Self {
        Self {
            file: autostart_dir.join(format!("{identifier}.desktop")),
            exec: exec_line(executable, args),
        }
    }

    /// The user's autostart folder: `$XDG_CONFIG_HOME/autostart`, else `~/.config/autostart`.
    pub fn user_dir() -> Option<PathBuf> {
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .filter(|dir| !dir.is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
        Some(config.join("autostart"))
    }

    /// The file a running Echo should be started from: the AppImage when inside one, else `current`.
    pub fn launch_target(current: PathBuf) -> PathBuf {
        std::env::var_os("APPIMAGE")
            .filter(|path| !path.is_empty())
            .map_or(current, PathBuf::from)
    }

    fn entry(&self) -> String {
        format!(
            "[Desktop Entry]\nType=Application\nName=Echo\nComment=Local, private speech-to-text\nExec={}\nIcon=echo\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
            self.exec
        )
    }
}

impl LaunchAtLogin for XdgAutostart {
    fn caps(&self) -> LaunchAtLoginCaps {
        LaunchAtLoginCaps { available: true }
    }

    fn state(&self) -> PortResult<LaunchAtLoginState> {
        let text = match fs::read_to_string(&self.file) {
            Ok(text) => text,
            Err(error) if error.kind() == ErrorKind::NotFound => {
                return Ok(LaunchAtLoginState::NotRegistered);
            }
            Err(error) => return Err(io_failure("read", &self.file, &error)),
        };
        Ok(LaunchAtLoginState::Registered {
            approved: approved(&text),
            current: entry_value(&text, "Exec").is_some_and(|exec| exec == self.exec),
        })
    }

    fn register(&self) -> PortResult<()> {
        if let Some(dir) = self.file.parent() {
            fs::create_dir_all(dir).map_err(|error| io_failure("create", dir, &error))?;
        }
        fs::write(&self.file, self.entry()).map_err(|error| io_failure("write", &self.file, &error))
    }

    fn unregister(&self) -> PortResult<()> {
        match fs::remove_file(&self.file) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
            Err(error) => Err(io_failure("remove", &self.file, &error)),
        }
    }
}

/// The value of `key` in the `[Desktop Entry]` group.
fn entry_value<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    let mut in_entry = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
        } else if in_entry
            && let Some((name, value)) = line.split_once('=')
            && name.trim() == key
        {
            return Some(value.trim());
        }
    }
    None
}

/// Off when the desktop hid the entry or switched GNOME's flag off.
fn approved(text: &str) -> bool {
    let is_true =
        |key| entry_value(text, key).is_some_and(|value| value.eq_ignore_ascii_case("true"));
    let is_false =
        |key| entry_value(text, key).is_some_and(|value| value.eq_ignore_ascii_case("false"));
    !is_true("Hidden") && !is_false("X-GNOME-Autostart-enabled")
}

/// `executable` and `args` as a Desktop Entry Exec value: each argument quoted, with `"`, `` ` ``, `$` and `\`
/// escaped, and `%` doubled (field codes).
fn exec_line(executable: &Path, args: &[&str]) -> String {
    std::iter::once(executable.to_string_lossy().into_owned())
        .chain(args.iter().map(|arg| (*arg).to_owned()))
        .map(|arg| {
            let mut quoted = String::from("\"");
            for ch in arg.chars() {
                match ch {
                    '"' | '`' | '$' | '\\' => {
                        quoted.push('\\');
                        quoted.push(ch);
                    }
                    '%' => quoted.push_str("%%"),
                    _ => quoted.push(ch),
                }
            }
            quoted.push('"');
            quoted
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn io_failure(action: &str, path: &Path, error: &std::io::Error) -> PortError {
    PortError::new(AppError::Internal)
        .with_detail(format!("could not {action} {}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("echo-autostart-{name}-{}", std::process::id()));
        drop(fs::remove_dir_all(&dir));
        dir
    }

    #[test]
    fn exec_lines_quote_and_escape() {
        assert_eq!(
            exec_line(Path::new("/opt/My Apps/echo"), &["--minimized"]),
            r#""/opt/My Apps/echo" "--minimized""#
        );
        assert_eq!(exec_line(Path::new("/a$b/100%"), &[]), r#""/a\$b/100%%""#);
    }

    #[test]
    fn register_state_and_unregister_round_trip() {
        let dir = scratch("round-trip");
        let entry = XdgAutostart::new(
            &dir,
            "app.echo.desktop",
            Path::new("/usr/bin/echo-app"),
            &["--minimized"],
        );
        assert_eq!(entry.state().unwrap(), LaunchAtLoginState::NotRegistered);
        entry.register().unwrap();
        assert_eq!(
            entry.state().unwrap(),
            LaunchAtLoginState::Registered {
                approved: true,
                current: true
            }
        );
        let moved = XdgAutostart::new(
            &dir,
            "app.echo.desktop",
            Path::new("/opt/echo"),
            &["--minimized"],
        );
        assert_eq!(
            moved.state().unwrap(),
            LaunchAtLoginState::Registered {
                approved: true,
                current: false
            }
        );
        entry.unregister().unwrap();
        entry.unregister().unwrap();
        assert_eq!(entry.state().unwrap(), LaunchAtLoginState::NotRegistered);
        drop(fs::remove_dir_all(&dir));
    }

    #[test]
    fn a_desktop_switch_off_is_not_approved_and_register_clears_it() {
        let dir = scratch("switched-off");
        let entry = XdgAutostart::new(
            &dir,
            "app.echo.desktop",
            Path::new("/usr/bin/echo-app"),
            &[],
        );
        entry.register().unwrap();
        let text = fs::read_to_string(&entry.file).unwrap();
        fs::write(
            &entry.file,
            text.replace(
                "X-GNOME-Autostart-enabled=true",
                "X-GNOME-Autostart-enabled=false",
            ),
        )
        .unwrap();
        assert!(!entry.state().unwrap().starts_at_login());
        fs::write(&entry.file, "[Desktop Entry]\nHidden=true\n").unwrap();
        assert!(!entry.state().unwrap().starts_at_login());
        entry.register().unwrap();
        assert!(entry.state().unwrap().starts_at_login());
        drop(fs::remove_dir_all(&dir));
    }
}
