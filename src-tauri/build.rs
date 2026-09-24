/*!
 * SOURCE OF TRUTH KEYWORDS: build.rs, tauri-build, build script, context generation, windows resources, app manifest, Common-Controls v6, test binaries
 * WHAT:  Cargo build script: runs tauri-build to embed tauri.conf.json, capabilities and the Windows icon, and
 *        embeds the Windows application manifest (Common-Controls v6) into every binary the crate links.
 * WHY:   `tauri::generate_context!` in app/ needs the artifacts tauri-build generates at compile time.
 *        tauri-build's own manifest is linked into the app binary only, so a test binary that instantiates the
 *        Wry runtime fails to start with STATUS_ENTRYPOINT_NOT_FOUND (comctl32 v6 imports, 05 W26). The manifest
 *        is therefore handed to the MSVC linker for all targets instead, and tauri-build's copy is turned off so
 *        the app binary does not end up with two.
 * WHERE: Run by cargo before compiling the crate.
 */

use std::error::Error;

const COMMON_CONTROLS_V6: &str = "/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' \
     version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'";

fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let windows = tauri_build::WindowsAttributes::new_without_app_manifest();
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))?;

    if std::env::var("CARGO_CFG_TARGET_ENV").is_ok_and(|env| env == "msvc") {
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg={COMMON_CONTROLS_V6}");
    }
    Ok(())
}
