/*!
 * SOURCE OF TRUTH KEYWORDS: app layer, composition root, run, tauri builder, generate_context, startup
 * WHAT:  Layer 7: the composition root. Builds the Tauri app from tauri.conf.json and runs its event loop.
 * WHY:   Only app/ wires Tauri plugins, windows and managed state (02 §3.2), so every other layer stays free of
 *        Tauri setup. Both windows (main, pill) are declared in tauri.conf.json and created at startup; the
 *        pill starts hidden so showing it later costs no window creation (02 §6.2).
 * WHERE: Called by main.rs; may import every layer.
 */

mod bindings;
mod bootstrap;
mod events;
mod logging;
mod panics;
mod plugins;
mod tray;
mod windows;

use std::process::ExitCode;

use crate::pipeline::launch;

/**
 * SOURCE OF TRUTH KEYWORDS: run, app entry, exit code, startup failure, invoke handler, mount events, run_return, RunEvent Ready, RunEvent Exit, launch origin, tray on ready
 * WHAT:  Builds the Tauri app, mounts the event catalog, runs the bootstrap sequence, then runs the event loop
 *        until exit; returns the loop's exit code, or failure if Echo could not start.
 * WHY:   Returns an ExitCode instead of panicking (denied) or calling process::exit, and uses `run_return`, so
 *        destructors run and the database closes cleanly. Plugins are added by app/plugins.rs before the build. The IPC surface comes from the same tauri-specta
 *        builder that generates src/bindings.ts. Events are mounted and the CommandCtx managed on the built app,
 *        before the event loop creates the windows, so no command or emit can run without them. A failure before
 *        logging exists goes to stderr; after that bootstrap has also written it to the log.
 *        Native window appearance (Mica, theme) is applied on RunEvent::Ready, the first moment the config
 *        windows exist; then the tray icon is built, the main window is shown unless Echo started at sign-in in the
 *        tray (always shown when the tray could not be built, so Echo is never unreachable), the startup recovery
 *        toast is shown (once), the session binds its hotkeys, and the speech engine and warm-up are scheduled a
 *        moment later (05 W19), so the UI is never held up by them, and the automatic update check later still (a no-op
 *        without an update source, 02 §11); on RunEvent::Exit the session finalizes an
 *        open recording before the process ends (02 §5); window events (close → hide) go to app/windows.rs. How
 *        Echo was started (the `--minimized` sign-in argument) is read once from the command line.
 * WHERE: Called once by main.rs.
 */
pub fn run() -> ExitCode {
    let ipc = bindings::builder::<tauri::Wry>();
    let app = match plugins::register(tauri::Builder::default())
        .invoke_handler(ipc.invoke_handler())
        .on_window_event(windows::on_window_event)
        .build(tauri::generate_context!())
    {
        Ok(app) => app,
        Err(error) => {
            eprintln!("Echo could not start: {error}");
            return ExitCode::FAILURE;
        }
    };
    ipc.mount_events(&app);
    let mut recovered = match bootstrap::start(&app) {
        Ok(report) => Some(report),
        Err(error) => {
            eprintln!("Echo could not start: {error}");
            return ExitCode::FAILURE;
        }
    };
    let origin = launch::origin_from_args(std::env::args());
    let code = app.run_return(move |handle, event| match event {
        tauri::RunEvent::Ready => {
            windows::setup(handle);
            let tray_ready = tray::create(handle)
                .inspect_err(|error| {
                    tracing::error!(%error, "the tray icon could not be created; the window stays open");
                })
                .is_ok();
            windows::show_at_launch(handle, origin, tray_ready);
            if let Some(report) = recovered.take() {
                bootstrap::announce_recovery(handle, report);
            }
            bootstrap::prepare_session(handle);
            bootstrap::schedule_warm_up(handle);
            bootstrap::schedule_update_check(handle);
        }
        tauri::RunEvent::Exit => bootstrap::stop_session(handle),
        _ => {}
    });
    ExitCode::from(u8::try_from(code).unwrap_or(u8::MAX))
}

/**
 * SOURCE OF TRUTH KEYWORDS: window config test, pill window contract, main window contract, CSP test, NSIS per-user test, version sync test, installer icon test, no updater artifacts
 * WHAT:  Checks the compiled tauri.conf.json against the specs the docs own: window sizes and flags (04 §4–5),
 *        the pill's hidden non-activating setup (05 W3), the no-remote CSP (02 §10), identifier and installer (02 §2.1,
 *        §11: per-user NSIS, embedded WebView2 bootstrapper, the app icon on installer and uninstaller, a stable
 *        publisher, no updater plugin or updater artifacts), and one version across tauri.conf.json, Cargo.toml and
 *        package.json (02 §11).
 * WHY:   The P0 exit gate is "a main window and a hidden pill"; a config edit that breaks it (a focusable pill
 *        steals the paste target) would otherwise only show up by hand. Window and CSP checks read the context
 *        `run` compiles in; bundle checks parse the file, because bundler settings are not compiled in. The
 *        publisher names the registry keys the installer upgrades in place, so changing it would orphan an install
 *        (05 W21). Versions are kept in sync by hand at each release, so the gate catches a missed file.
 * WHERE: `cargo test` (local gate).
 */
#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use tauri::utils::config::{
        BundleResources, BundleTarget, BundleType, Csp, CspDirectiveSources, NSISInstallerMode,
        Updater, WebviewInstallMode, WindowConfig,
    };

    fn config() -> tauri::Config {
        let context: tauri::Context<tauri::Wry> = tauri::generate_context!();
        context.config().clone()
    }

    fn window(config: &tauri::Config, label: &str) -> WindowConfig {
        config
            .app
            .windows
            .iter()
            .find(|window| window.label == label)
            .cloned()
            .unwrap_or_else(|| panic!("tauri.conf.json declares no `{label}` window"))
    }

    fn sources(csp: &Csp) -> Vec<(String, String)> {
        HashMap::<String, CspDirectiveSources>::from(csp.clone())
            .into_iter()
            .flat_map(|(directive, sources)| {
                Vec::<String>::from(sources)
                    .into_iter()
                    .map(move |source| (directive.clone(), source))
            })
            .collect()
    }

    fn file() -> tauri::Config {
        serde_json::from_str(include_str!("../../tauri.conf.json"))
            .expect("tauri.conf.json parses as a Tauri config")
    }

    #[test]
    fn identifier_and_product_name_are_fixed() {
        let config = config();
        assert_eq!(config.identifier, "app.echo.desktop");
        assert_eq!(config.product_name.as_deref(), Some("Echo"));
    }

    #[test]
    fn main_window_matches_the_design_spec() {
        let main = window(&config(), "main");
        assert_eq!((main.width, main.height), (960.0, 640.0));
        assert_eq!(
            (main.min_width, main.min_height),
            (Some(820.0), Some(560.0))
        );
        assert!(!main.decorations, "the main window draws its own titlebar");
        assert!(
            main.transparent,
            "Mica shows only through a transparent window (04 §2)"
        );
        assert!(
            !main.visible,
            "app/windows.rs shows it at launch unless Echo starts in the tray"
        );
    }

    #[test]
    fn pill_window_is_precreated_hidden_and_never_takes_focus() {
        let pill = window(&config(), "pill");
        assert_eq!((pill.width, pill.height), (360.0, 88.0));
        assert!(!pill.visible, "the pill is created hidden at startup");
        assert!(pill.transparent);
        assert!(pill.always_on_top);
        assert!(pill.skip_taskbar);
        assert!(!pill.decorations);
        assert!(
            !pill.focus,
            "focus would move the paste target away from the user's app"
        );
        assert!(!pill.focusable);
    }

    #[test]
    fn csp_allows_no_remote_origin() {
        let security = config().app.security;
        let production = security.csp.expect("a production CSP is configured");
        let development = security.dev_csp.expect("a development CSP is configured");
        let local = ["'self'", "'none'", "ipc:", "http://ipc.localhost"];

        for (directive, source) in sources(&production) {
            assert!(
                local.contains(&source.as_str()),
                "production CSP {directive} allows {source}"
            );
        }
        for (directive, source) in sources(&development) {
            let dev_only = ["'unsafe-inline'", "ws://localhost:1420"];
            assert!(
                local.contains(&source.as_str()) || dev_only.contains(&source.as_str()),
                "development CSP {directive} allows {source}"
            );
        }
    }

    /// Bundle settings are read by the Tauri CLI, not compiled into the context, so they are parsed from the file.
    #[test]
    fn installer_is_per_user_nsis_with_embedded_webview_bootstrapper() {
        let bundle = file().bundle;
        assert_eq!(bundle.targets, BundleTarget::List(vec![BundleType::Nsis]));
        let nsis = bundle.windows.nsis.expect("NSIS settings are configured");
        assert_eq!(nsis.install_mode, NSISInstallerMode::CurrentUser);
        assert!(matches!(
            bundle.windows.webview_install_mode,
            WebviewInstallMode::EmbedBootstrapper { .. }
        ));
    }

    /// The installer, the uninstaller and the executable all carry the app icon, and the publisher never changes.
    #[test]
    fn installer_carries_the_app_icon_and_a_stable_publisher() {
        let bundle = file().bundle;
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let app_icon = std::path::PathBuf::from("icons/icon.ico");
        assert!(bundle.icon.iter().any(|icon| *icon == "icons/icon.ico"));
        let nsis = bundle.windows.nsis.expect("NSIS settings are configured");
        for icon in [nsis.installer_icon, nsis.uninstaller_icon] {
            assert_eq!(icon.as_ref(), Some(&app_icon));
        }
        assert!(
            manifest_dir.join(&app_icon).is_file(),
            "icons/icon.ico exists"
        );
        assert_eq!(bundle.publisher.as_deref(), Some("Echo"));
    }

    /// Updates are disabled (02 §11): no updater plugin, endpoint or signed update artifacts.
    #[test]
    fn no_updater_is_configured() {
        let file = file();
        assert!(!file.plugins.0.contains_key("updater"));
        assert!(matches!(
            file.bundle.create_updater_artifacts,
            Updater::Bool(false)
        ));
    }

    #[test]
    fn versions_agree_across_tauri_cargo_and_package_json() {
        let package: serde_json::Value =
            serde_json::from_str(include_str!("../../../package.json"))
                .expect("package.json parses");
        let cargo = env!("CARGO_PKG_VERSION");
        assert_eq!(config().version.as_deref(), Some(cargo));
        assert_eq!(package["version"].as_str(), Some(cargo));
    }

    /// AppPaths reads bundled files at the same relative path they have under src-tauri/resources (ONNX Runtime,
    /// bundled models), so every resource folder must map onto a folder of the same name. The typefaces' OFL texts
    /// live beside the fonts in src/styles/fonts and are installed into `licenses/` one by one (registry/credits.rs).
    #[test]
    fn bundled_resources_keep_their_folder_layout() {
        let Some(BundleResources::Map(resources)) = file().bundle.resources else {
            panic!("bundle.resources must map resource folders to install folders");
        };
        let mut folders: Vec<_> = resources.into_iter().collect();
        folders.sort();
        assert_eq!(
            folders,
            [
                (
                    "../src/styles/fonts/Inter-OFL.txt",
                    "licenses/Inter-OFL.txt"
                ),
                (
                    "../src/styles/fonts/Poppins-OFL.txt",
                    "licenses/Poppins-OFL.txt"
                ),
                ("resources/licenses/*", "licenses/"),
                ("resources/models/*", "models/"),
                ("resources/onnxruntime/*", "onnxruntime/"),
            ]
            .map(|(from, to)| (from.to_owned(), to.to_owned()))
        );
    }
}
