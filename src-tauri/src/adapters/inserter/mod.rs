/*!
 * SOURCE OF TRUTH KEYWORDS: inserter adapters, TextInserter implementations, Win32SendInputInserter, paste adapter
 * WHAT:  Adapters behind the TextInserter port.
 * WHY:   Synthetic keyboard input is a Windows API concern and stays behind its port (root CLAUDE.md §3); a
 *        typing or UI Automation inserter would be a second adapter here, picked by its caps.
 * WHERE: Constructed by app/bootstrap; used only through `dyn TextInserter` by pipeline/delivery.rs.
 */

mod win32_send_input;

pub use win32_send_input::{SYNTHETIC_INPUT_TAG, Win32SendInputInserter};
