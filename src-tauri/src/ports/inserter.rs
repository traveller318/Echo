/*!
 * SOURCE OF TRUTH KEYWORDS: TextInserter, paste, insert text, SendInput, Ctrl+V, target window, elevated target, UIPI
 * WHAT:  TextInserter puts the finished text into the app the take started in.
 * WHY:   The MVP adapter pastes (Ctrl+V after the hotkey's modifiers are released, 05 W1) and declares
 *        `InserterCaps.uses_clipboard`, so delivery writes the clipboard first. Passing the text as well lets a
 *        later adapter type it or set it through UI Automation without touching the pipeline. Elevated targets
 *        are checked by the pipeline from `AppTarget.elevated` and `InserterCaps.can_target_elevated` before
 *        calling (05 W2); an adapter that still finds itself blocked fails with
 *        `PermissionDenied { input_injection }`. The call blocks while it waits for modifiers (up to ~400 ms).
 * WHERE: Implemented by adapters/inserter/win32_send_input.rs (Win32SendInputInserter) and ports/fakes; used by
 *        pipeline/delivery.rs.
 */

use crate::types::{AppTarget, InserterCaps, PortResult};

/// Delivers text into another app.
pub trait TextInserter: Send + Sync {
    fn caps(&self) -> InserterCaps;

    /// Inserts `text` into `target`.
    fn insert(&self, target: &AppTarget, text: &str) -> PortResult<()>;
}
