/*!
 * SOURCE OF TRUTH KEYWORDS: Toast, ToastKind, native notification, Windows toast, notifier message
 * WHAT:  Toast: one native notification (kind, title, body). ToastKind: its severity.
 * WHY:   Toasts cover the moments the pill cannot: copy-only fallback (05 W2), device lost (05 W12), recovered takes
 *        (02 §7.3). A struct instead of positional arguments lets a later notifier add fields (an action button)
 *        without changing the port. Text is StaticStr so fixed toasts are `const`, while counted ones
 *        ("Recovered 2 takes") are built owned. Toast text is calm copy (04 §1) and never contains transcript text.
 * WHERE: `Notifier::toast` (ports/notifier.rs); built by the pipeline (delivery, recovery, power handling).
 */

use super::StaticStr;

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
