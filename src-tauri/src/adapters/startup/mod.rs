/*!
 * SOURCE OF TRUTH KEYWORDS: startup adapters, LaunchAtLogin implementations, Win32RunKey, DisabledLaunchAtLogin, launch at sign-in
 * WHAT:  Adapters behind the LaunchAtLogin port: the user's Run key (installed builds) and a disabled one
 *        (development builds).
 * WHY:   A development build's executable loads its pages from the dev server, so registering it would start a
 *        blank Echo at every sign-in; app/bootstrap picks the disabled adapter there, whose caps hide the startup
 *        settings, and the core never asks which build it is.
 * WHERE: Constructed by app/bootstrap; used only through `dyn LaunchAtLogin`.
 */

mod disabled;
mod run_key;

pub use disabled::DisabledLaunchAtLogin;
pub use run_key::{RunKeyLocation, Win32RunKey};
