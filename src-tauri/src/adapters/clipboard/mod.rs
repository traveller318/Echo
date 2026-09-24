/*!
 * SOURCE OF TRUTH KEYWORDS: clipboard adapters, Clipboard implementations, ArboardClipboard
 * WHAT:  Adapters behind the Clipboard port.
 * WHY:   The system clipboard is an OS concern and stays behind its port (root CLAUDE.md §3).
 * WHERE: Constructed by app/bootstrap; used only through `dyn Clipboard` by pipeline/delivery.rs.
 */

mod arboard;

pub use self::arboard::ArboardClipboard;
