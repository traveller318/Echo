/*!
 * SOURCE OF TRUTH KEYWORDS: DeliveryPolicy, DeliveryPlan, CopyReason, DeliveryReport, ClipboardRestore, auto paste, keep on clipboard, copy-only fallback
 * WHAT:  The shapes of delivering a finished take: the policy the `output.*` settings give (DeliveryPolicy), what
 *        delivery decided to do (DeliveryPlan, and CopyReason when it only copies), and what happened
 *        (DeliveryReport, with a ClipboardRestore when the user's previous clipboard is to be given back).
 * WHY:   Delivery decisions are pure data so they are table-tested without a desktop (02 §13), and the session
 *        actor can store and show the outcome without re-deriving it. The copy reason is kept apart from the
 *        outcome because the pill only needs "copied" (DeliveryOutcome) while the log and the toast need why.
 *        ClipboardRestore carries transcript text, so its Debug prints lengths only: logs never contain transcript
 *        text (02 §10). Restoring waits ≥ 300 ms after the paste (05 W6), so it is handed back to the caller
 *        instead of blocking delivery.
 * WHERE: Built by registry::settings::delivery_policy and pipeline/delivery.rs; consumed by the session actor
 *        (step 14) and the paste-last / history-copy paths.
 */

use std::fmt;

use super::{AppTarget, DeliveryOutcome};

/// What the `output.*` settings ask delivery to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DeliveryPolicy {
    /// `output.auto_paste`: paste into the app the take started in.
    pub auto_paste: bool,
    /// `output.keep_on_clipboard`: leave the text on the clipboard after a paste; off gives the previous
    /// clipboard back. A copy-only delivery always keeps it, since the clipboard is then the delivery.
    pub keep_on_clipboard: bool,
}

/// Why a take was copied instead of pasted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CopyReason {
    /// `output.auto_paste` is off: copying is what the user asked for.
    AutoPasteOff,
    /// No window had focus when the take started (the desktop, a lock screen).
    NoTarget,
    /// The target runs at a higher integrity level and the inserter cannot reach it (05 W2).
    ElevatedTarget,
    /// The inserter tried and failed (the window closed or could not be brought back to the front).
    PasteFailed,
}

/// What delivery will do before it touches the clipboard.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DeliveryPlan {
    /// Nothing to deliver: the text is empty (a silence-only or filler-only take, 05 A4).
    Nothing,
    /// Insert into the target.
    Paste(AppTarget),
    /// Put the text on the clipboard only.
    Copy(CopyReason),
}

/// The user's clipboard as it was before a paste, to be given back once the paste has landed (05 W6).
#[derive(Clone, PartialEq, Eq)]
pub struct ClipboardRestore {
    /// The clipboard's text before delivery; None when it held no text (it is then cleared instead).
    pub previous: Option<String>,
    /// What delivery wrote; the restore is skipped when the clipboard no longer holds exactly this.
    pub delivered: String,
}

impl fmt::Debug for ClipboardRestore {
    /// Lengths only: this carries transcript and clipboard text, which never reaches a log (02 §10).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClipboardRestore")
            .field("previous_chars", &self.previous.as_ref().map(String::len))
            .field("delivered_chars", &self.delivered.len())
            .finish()
    }
}

/// What delivering a take did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveryReport {
    pub outcome: DeliveryOutcome,
    /// Set when `outcome` is `Copied`.
    pub copy_reason: Option<CopyReason>,
    /// Set after a paste while `keep_on_clipboard` is off: run it after the restore delay.
    pub restore: Option<ClipboardRestore>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restore_debug_never_prints_text() {
        let restore = ClipboardRestore {
            previous: Some(String::from("secret before")),
            delivered: String::from("private dictation"),
        };
        let printed = format!("{restore:?}");
        assert!(!printed.contains("secret"), "{printed}");
        assert!(!printed.contains("private"), "{printed}");
        assert!(
            printed.contains("13") && printed.contains("17"),
            "{printed}"
        );
    }
}
