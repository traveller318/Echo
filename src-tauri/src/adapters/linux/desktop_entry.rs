/*!
 * SOURCE OF TRUTH KEYWORDS: desktop entry, app info not found, portal app id, NoDisplay desktop file, XDG_DATA_DIRS lookup, AppImage desktop integration, generated desktop entry marker
 * WHAT:  `ensure`: makes sure a desktop file named after Echo's app id exists where the desktop looks for it, and writes
 *        a hidden one in the user's applications folder when none does. `find_in` / `entry_text` are its pure parts.
 * WHY:   xdg-desktop-portal turns "this process is app.echo.desktop" into a known app only if `<app id>.desktop` exists
 *        in an applications folder; without it Register fails with "App info not found" and every portal permission
 *        and shortcut is filed under no app at all (Hyprland shows `:record` instead of `app.echo.desktop:record`).
 *        A .deb or .rpm installs the file, but a development build and an AppImage have none, so Echo writes one on
 *        first need. It is `NoDisplay=true` (a lookup target, never a menu entry), points at the file that is
 *        running (the AppImage itself when there is one), and carries an `X-Echo-Generated` marker: only a file with
 *        that marker is ever rewritten (when the executable moved), so a packaged or hand-made entry is left alone.
 *        A folder that cannot be written is only logged: portals then work unregistered, as before.
 * WHERE: adapters/linux/portal.rs `register_app`, before the first portal call.
 */

use std::{
    fs,
    path::{Path, PathBuf},
};

/// The marker line that makes a desktop file Echo's own to update.
const MARKER: &str = "X-Echo-Generated=true";

/// Makes `<app_id>.desktop` findable, creating the hidden user entry when no applications folder has it.
pub fn ensure(app_id: &str) {
    let Ok(executable) = std::env::current_exe() else {
        return;
    };
    let launch = std::env::var_os("APPIMAGE")
        .filter(|path| !path.is_empty())
        .map_or(executable, PathBuf::from);
    let home_data = std::env::var_os("XDG_DATA_HOME")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")));
    let system_data =
        std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".to_owned());
    let mut dirs: Vec<PathBuf> = home_data.iter().cloned().collect();
    dirs.extend(std::env::split_paths(&system_data));
    let Some(home_data) = home_data else {
        return;
    };
    if let Err(error) = write_if_missing(&dirs, &home_data, app_id, &launch) {
        tracing::warn!(%error, app_id, "Echo's desktop entry could not be written; desktop permissions may not be remembered");
    }
}

/// Writes the entry into `<home_data>/applications` unless some folder in `dirs` already has a file that is not ours.
fn write_if_missing(
    dirs: &[PathBuf],
    home_data: &Path,
    app_id: &str,
    launch: &Path,
) -> std::io::Result<()> {
    let ours = home_data
        .join("applications")
        .join(format!("{app_id}.desktop"));
    if let Some(found) = find_in(dirs, app_id) {
        let generated = found == ours && fs::read_to_string(&found)?.contains(MARKER);
        if !generated {
            return Ok(());
        }
    }
    let wanted = entry_text(launch);
    if fs::read_to_string(&ours).is_ok_and(|current| current == wanted) {
        return Ok(());
    }
    if let Some(folder) = ours.parent() {
        fs::create_dir_all(folder)?;
    }
    fs::write(&ours, wanted)
}

/// The first `<dir>/applications/<app_id>.desktop` that exists.
fn find_in(dirs: &[PathBuf], app_id: &str) -> Option<PathBuf> {
    dirs.iter()
        .map(|dir| dir.join("applications").join(format!("{app_id}.desktop")))
        .find(|candidate| candidate.is_file())
}

/// The hidden entry for the executable `launch`; the Exec path is quoted per the Desktop Entry spec.
fn entry_text(launch: &Path) -> String {
    let path = launch.to_string_lossy();
    let escaped: String = path
        .chars()
        .flat_map(|ch| match ch {
            '"' | '`' | '$' | '\\' => vec!['\\', ch],
            '%' => vec!['%', '%'],
            other => vec![other],
        })
        .collect();
    format!(
        "[Desktop Entry]\nType=Application\nName=Echo\nComment=Local, private speech-to-text\nExec=\"{escaped}\"\nTerminal=false\nNoDisplay=true\nCategories=AudioVideo;Utility;\n{MARKER}\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("echo-desktop-{name}-{}", std::process::id()));
        drop(fs::remove_dir_all(&dir));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_missing_entry_is_written_hidden_and_marked() {
        let home = scratch("missing");
        write_if_missing(
            std::slice::from_ref(&home),
            &home,
            "app.echo.desktop",
            Path::new("/opt/Echo App/echo"),
        )
        .unwrap();
        let text = fs::read_to_string(home.join("applications/app.echo.desktop.desktop")).unwrap();
        assert!(text.contains("NoDisplay=true") && text.contains(MARKER));
        assert!(text.contains("Exec=\"/opt/Echo App/echo\""));
        drop(fs::remove_dir_all(&home));
    }

    #[test]
    fn a_packaged_entry_is_never_touched_and_ours_follows_the_executable() {
        let home = scratch("packaged");
        let system = scratch("system");
        fs::create_dir_all(system.join("applications")).unwrap();
        let packaged = system.join("applications/app.echo.desktop.desktop");
        fs::write(&packaged, "[Desktop Entry]\nName=Echo\n").unwrap();
        let dirs = [home.clone(), system.clone()];
        write_if_missing(&dirs, &home, "app.echo.desktop", Path::new("/a/echo")).unwrap();
        assert!(!home.join("applications/app.echo.desktop.desktop").exists());
        assert_eq!(
            fs::read_to_string(&packaged).unwrap(),
            "[Desktop Entry]\nName=Echo\n"
        );

        let only_home = [home.clone()];
        write_if_missing(&only_home, &home, "app.echo.desktop", Path::new("/a/echo")).unwrap();
        write_if_missing(&only_home, &home, "app.echo.desktop", Path::new("/b/echo")).unwrap();
        let ours = fs::read_to_string(home.join("applications/app.echo.desktop.desktop")).unwrap();
        assert!(ours.contains("/b/echo") && !ours.contains("/a/echo"));

        // Without the marker the file belongs to the user, even in our own folder.
        let theirs = home.join("applications/app.echo.desktop.desktop");
        fs::write(&theirs, "[Desktop Entry]\nName=Mine\n").unwrap();
        write_if_missing(&only_home, &home, "app.echo.desktop", Path::new("/c/echo")).unwrap();
        assert_eq!(
            fs::read_to_string(&theirs).unwrap(),
            "[Desktop Entry]\nName=Mine\n"
        );
        drop(fs::remove_dir_all(&home));
        drop(fs::remove_dir_all(&system));
    }

    #[test]
    fn exec_paths_are_escaped() {
        assert!(entry_text(Path::new("/a$b/100%")).contains("Exec=\"/a\\$b/100%%\""));
    }
}
