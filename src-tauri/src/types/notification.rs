/*!
 * SOURCE OF TRUTH KEYWORDS: Toast, ToastKind, OneTimeNotice, native notification, Windows toast, notifier message
 * WHAT:  Toast: one native notification (kind, title, body). ToastKind: its severity.
 * WHY:   Toasts cover the moments the pill cannot: copy-only fallback (05 W2), device lost (05 W12), recovered takes
 *        (02 §7.3). A struct instead of positional arguments lets a later notifier add fields (an action button)
 *        without changing the port. Text is StaticStr so fixed toasts are `const`, while counted ones
 *        ("Recovered 2 takes") are built owned. Toast text is calm copy (04 §1) and never contains transcript text.
 * WHERE: `Notifier::toast` (ports/notifier.rs); built by the pipeline (delivery, recovery, power handling).
 */

use super::{SettingKey, StaticStr};

/// How a toast is presented.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToastKind {
    Info,
    Success,
    Warning,
    Error,
}

/// One native notification.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Toast {
    pub kind: ToastKind,
    pub title: StaticStr,
    pub body: StaticStr,
}

/**
 * SOURCE OF TRUTH KEYWORDS: OneTimeNotice, show once, hint shown flag, first-time hint, notice registry entry
 * WHAT:  A toast Echo shows at most once per installation, and the hidden Bool setting that remembers it was shown.
 * WHY:   Some hints only help the first time (a Bluetooth microphone cuts the first second, 05 W11); repeating them
 *        on every take would be noise. The "shown" flag is a hidden setting (`visible: false`, 02 §3.3), so it lives
 *        in the one settings table, survives restarts and is reset like any setting; no second store exists.
 * WHERE: Entries in registry/notices.rs; shown by pipeline/notices.rs (NoticeBoard::show_once).
 */
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OneTimeNotice {
    /// Hidden Bool setting, false until the notice has been shown.
    pub shown: SettingKey,
    pub toast: Toast,
}
