/*!
 * SOURCE OF TRUTH KEYWORDS: Clipboard, read clipboard, write clipboard, clear clipboard, clipboard history exclusion, clipboard busy, retry
 * WHAT:  Clipboard reads, writes and clears plain text on the system clipboard, optionally excluded from Windows
 *        clipboard history.
 * WHY:   Delivery puts every transcript on the clipboard (the paste reads it, and it stays there by default).
 *        `read_text` and `clear` exist for giving the user's previous clipboard back after a paste while
 *        `output.keep_on_clipboard` is off (05 W6): previous text is written back, anything else (an image, an
 *        empty clipboard) cannot be carried by a text port, so the transcript is cleared instead of left behind.
 *        Paste-last reads History, never the clipboard. Another app holding the clipboard is normal (05 W4), so
 *        adapters retry internally and only then fail with `PermissionDenied { clipboard }`, keeping retry policy
 *        out of the pipeline. Calls block for at most that retry budget.
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

    /// Empties the clipboard.
    fn clear(&self) -> PortResult<()>;
}
