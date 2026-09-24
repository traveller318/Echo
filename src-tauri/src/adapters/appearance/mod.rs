/*!
 * SOURCE OF TRUTH KEYWORDS: appearance adapters, SystemAppearance implementations, Win32SystemAppearance, transparency effects
 * WHAT:  Adapters behind the SystemAppearance port.
 * WHY:   Windows personalization settings and the OS build are Windows API concerns and stay behind their port
 *        (root CLAUDE.md §3).
 * WHERE: Constructed by app/bootstrap; used only through `dyn SystemAppearance`.
 */

mod win32;

pub use win32::Win32SystemAppearance;
