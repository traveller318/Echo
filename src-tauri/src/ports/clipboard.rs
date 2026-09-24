/*!
 * SOURCE OF TRUTH KEYWORDS: Clipboard, read clipboard, write clipboard, clipboard history exclusion, clipboard busy, retry
 * WHAT:  Clipboard reads and writes plain text on the system clipboard, optionally excluded from Windows
 *        clipboard history.
 * WHY:   Delivery puts every transcript on the clipboard (the paste reads it, and it stays there by default).
 *        `read_text` exists for restoring the user's previous clipboard after a paste (05 W6); paste-last reads
 *        History, never the clipboard. Another app holding the clipboard is normal (05 W4), so adapters retry
 *        internally and only then fail with `PermissionDenied { clipboard }`, keeping retry policy out of the
 *        pipeline. Calls block for at most that retry budget.
 * WHERE: Implemented by adapters/clipboard/arboard.rs (ArboardClipboard) and ports/fakes; used by
 *        pipeline/delivery.rs.
 */

use crate::types::{ClipboardHistory, PortResult};

/// The system clipboard.
pub trait Clipboard: Send + Sync {
    /// The clipboard's text, or None when it holds no text.
    fn read_text(&self) -> PortResult<Option<String>>;

    /// Replaces the clipboard content with `text`.
    fn write_text(&self, text: &str, history: ClipboardHistory) -> PortResult<()>;
}
