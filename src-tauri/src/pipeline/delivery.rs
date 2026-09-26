/*!
 * SOURCE OF TRUTH KEYWORDS: Delivery, deliver take, delivery plan, auto paste, keep on clipboard, copy-only fallback, elevated target toast, clipboard restore, CLIPBOARD_RESTORE_DELAY, keep_in_app
 * WHAT:  Delivery: puts a finished take's text where the user wants it. `plan` decides (nothing, paste, or copy
 *        and why) from the text, the take's target, the `output.*` policy and the inserter's caps; `deliver`
 *        carries the plan out through the Clipboard, TextInserter and Notifier ports and reports what happened;
 *        `copy` is a plain excluded clipboard write; `restore_clipboard` gives the user's previous clipboard back;
 *        `keep_in_app` is a rehearsed take's delivery (the text stays in Echo, no port is touched).
 * WHY:   One owner for 02 §9's paste rules. Every transcript write is excluded from Win+V history and cloud sync
 *        (05 W5). The clipboard is written first only when the inserter pastes from it (`uses_clipboard`, 05
 *        decision log). An elevated target that the inserter cannot reach is copied instead of pasted, with the
 *        toast "Couldn't paste into that window. Copied instead. Press Ctrl+V." (05 W2, 01 "clear failure
 *        toasts"); a paste that fails, or a take with no focused window, is copied the same way, so the text is
 *        always somewhere the user can paste it. When auto-paste is off, copying is the request and no toast is
 *        shown. A clipboard that stays busy after the adapter's retries is returned as an error (05 W4): the
 *        caller marks the take failed and toasts, and the text is still in History because the row is written
 *        before delivery (02 §7.3). A toast that cannot be shown is logged and never fails delivery. With
 *        `keep_on_clipboard` off, the previous clipboard text is read before the write and handed back in the
 *        report; the caller waits CLIPBOARD_RESTORE_DELAY (slow targets read the clipboard late, 05 W6) and calls
 *        `restore_clipboard`, which only acts while the clipboard still holds the delivered text, so it never
 *        overwrites something copied in the meantime. Previous content that was not text is cleared instead: the
 *        transcript is still not kept. Nothing here logs transcript text (02 §10).
 * WHERE: Built by app/bootstrap and held in CommandCtx; called by the session actor after polish (step 14),
 *        history_copy / history_paste_last (step 16) and the paste-last hotkey (through the session runner).
 */

use std::{sync::Arc, time::Duration};

use crate::{
    ports::{Clipboard, Notifier, TextInserter},
    types::{
        AppTarget, ClipboardHistory, ClipboardRestore, CopyReason, DeliveryOutcome, DeliveryPlan,
        DeliveryPolicy, DeliveryReport, InserterCaps, PortResult, StaticStr, Toast, ToastKind,
    },
};

/// How long after a paste the previous clipboard may be put back (05 W6).
pub const CLIPBOARD_RESTORE_DELAY: Duration = Duration::from_millis(300);

/// Shown when the target could not receive the paste: elevated (05 W2) or the paste failed.
pub const PASTE_BLOCKED_TOAST: Toast = Toast {
    kind: ToastKind::Warning,
    title: StaticStr::new("Couldn't paste into that window"),
    body: StaticStr::new("Copied instead. Press Ctrl+V."),
};

/// Shown when no window had focus when the take started.
pub const NO_TARGET_TOAST: Toast = Toast {
    kind: ToastKind::Info,
    title: StaticStr::new("No window to paste into"),
    body: StaticStr::new("Copied instead. Press Ctrl+V."),
};

/// The ports delivery works through.
pub struct DeliveryPorts {
    pub clipboard: Arc<dyn Clipboard>,
    pub inserter: Arc<dyn TextInserter>,
    pub notifier: Arc<dyn Notifier>,
}

/// Delivers finished text to the clipboard and the target app.
#[derive(Clone)]
pub struct Delivery {
    clipboard: Arc<dyn Clipboard>,
    inserter: Arc<dyn TextInserter>,
    notifier: Arc<dyn Notifier>,
}

/**
 * SOURCE OF TRUTH KEYWORDS: delivery plan, paste or copy decision, elevated target rule, no target rule
 * WHAT:  What delivering `text` will do, before any port is touched.
 * WHY:   Pure, so every combination of policy, target and caps is table-tested (02 §13). Empty text (silence or
 *        a filler-only take) delivers nothing, so the user's clipboard is never replaced by an empty string.
 * WHERE: Delivery::deliver; the session actor may call it to tell the pill early whether a paste is coming.
 */
pub fn plan(
    text: &str,
    target: Option<&AppTarget>,
    policy: DeliveryPolicy,
    caps: InserterCaps,
) -> DeliveryPlan {
    if is_nothing(text) {
        return DeliveryPlan::Nothing;
    }
    if !policy.auto_paste {
        return DeliveryPlan::Copy(CopyReason::AutoPasteOff);
    }
    match target {
        None => DeliveryPlan::Copy(CopyReason::NoTarget),
        Some(target) if target.elevated && !caps.can_target_elevated => {
            DeliveryPlan::Copy(CopyReason::ElevatedTarget)
        }
        Some(target) => DeliveryPlan::Paste(target.clone()),
    }
}

/// Text that delivers nothing: empty or whitespace only.
fn is_nothing(text: &str) -> bool {
    text.trim().is_empty()
}

/**
 * SOURCE OF TRUTH KEYWORDS: keep_in_app, rehearsed take delivery, practice take, DeliveryOutcome Shown
 * WHAT:  The report of a rehearsed take (onboarding's practice take): `Shown` when there is text, `NoSpeech` when
 *        there is none; the clipboard, the target app and toasts are never touched.
 * WHY:   The practice take proves the whole pipeline while the text is shown in Echo's card instead of pasted
 *        (step 24); it keeps the empty-text rule of `plan`, so an empty practice take reads "No speech detected".
 * WHERE: The session runner's delivery when the session rehearses a take in an Echo window
 *        (pipeline/session/rehearsal.rs decides).
 */
pub fn keep_in_app(text: &str) -> DeliveryReport {
    DeliveryReport {
        outcome: if is_nothing(text) {
            DeliveryOutcome::NoSpeech
        } else {
            DeliveryOutcome::Shown
        },
        copy_reason: None,
        restore: None,
    }
}

/// The toast a copy for `reason` shows; None when copying is what the user asked for.
pub fn copy_toast(reason: CopyReason) -> Option<Toast> {
    match reason {
        CopyReason::AutoPasteOff => None,
        CopyReason::NoTarget => Some(NO_TARGET_TOAST),
        CopyReason::ElevatedTarget | CopyReason::PasteFailed => Some(PASTE_BLOCKED_TOAST),
    }
}

impl Delivery {
    pub fn new(ports: DeliveryPorts) -> Self {
        let DeliveryPorts {
            clipboard,
            inserter,
            notifier,
        } = ports;
        Self {
            clipboard,
            inserter,
            notifier,
        }
    }

    /// Delivers `text` to `target` under `policy`; an error means the text reached neither the app nor the
    /// clipboard.
    pub fn deliver(
        &self,
        text: &str,
        target: Option<&AppTarget>,
        policy: DeliveryPolicy,
    ) -> PortResult<DeliveryReport> {
        let caps = self.inserter.caps();
        match plan(text, target, policy, caps) {
            DeliveryPlan::Nothing => Ok(DeliveryReport {
                outcome: DeliveryOutcome::NoSpeech,
                copy_reason: None,
                restore: None,
            }),
            DeliveryPlan::Copy(reason) => {
                self.copy(text)?;
                Ok(self.copied(reason))
            }
            DeliveryPlan::Paste(target) => self.paste(text, &target, policy, caps),
        }
    }

    /// Puts `text` on the clipboard, excluded from Windows clipboard history and cloud sync (05 W5).
    pub fn copy(&self, text: &str) -> PortResult<()> {
        self.clipboard.write_text(text, ClipboardHistory::Exclude)
    }

    /**
     * SOURCE OF TRUTH KEYWORDS: restore_clipboard, previous clipboard, keep_on_clipboard off, restore only if unchanged
     * WHAT:  Gives the clipboard its pre-delivery content back; true when it did, false when the clipboard had
     *        changed since delivery and was left alone.
     * WHY:   Run CLIPBOARD_RESTORE_DELAY after the paste (05 W6). Checking that the clipboard still holds the
     *        delivered text keeps a later copy (the user's or the next take's) from being overwritten. The restored
     *        text is written excluded too: it was archived when it was first copied, if its owner allowed that,
     *        and a password manager's excluded copy must not reach Win+V history through Echo.
     * WHERE: The session actor's restore timer (step 14).
     */
    pub fn restore_clipboard(&self, restore: &ClipboardRestore) -> PortResult<bool> {
        let current = self.clipboard.read_text()?;
        if current.as_deref() != Some(restore.delivered.as_str()) {
            return Ok(false);
        }
        match &restore.previous {
            Some(previous) => self
                .clipboard
                .write_text(previous, ClipboardHistory::Exclude)?,
            None => self.clipboard.clear()?,
        }
        Ok(true)
    }

    fn paste(
        &self,
        text: &str,
        target: &AppTarget,
        policy: DeliveryPolicy,
        caps: InserterCaps,
    ) -> PortResult<DeliveryReport> {
        let previous =
            (caps.uses_clipboard && !policy.keep_on_clipboard).then(|| self.previous_text());
        if caps.uses_clipboard {
            self.copy(text)?;
        }
        if let Err(error) = self.inserter.insert(target, text) {
            tracing::warn!(
                code = error.error().code().as_str(),
                detail = error.detail(),
                "the paste failed; the text is copied instead"
            );
            if !caps.uses_clipboard {
                self.copy(text)?;
            }
            return Ok(self.copied(CopyReason::PasteFailed));
        }
        Ok(DeliveryReport {
            outcome: DeliveryOutcome::Pasted,
            copy_reason: None,
            restore: previous.map(|previous| ClipboardRestore {
                previous,
                delivered: text.to_owned(),
            }),
        })
    }

    /// The clipboard's text before delivery; an unreadable clipboard counts as holding no text, so the restore
    /// clears it rather than leaving the transcript behind.
    fn previous_text(&self) -> Option<String> {
        self.clipboard.read_text().unwrap_or_else(|error| {
            tracing::warn!(
                detail = error.detail(),
                "the clipboard could not be read before the paste; it will be cleared instead of restored"
            );
            None
        })
    }

    /// The report of a copy for `reason`, after showing its toast.
    fn copied(&self, reason: CopyReason) -> DeliveryReport {
        if let Some(toast) = copy_toast(reason)
            && let Err(error) = self.notifier.toast(&toast)
        {
            tracing::warn!(
                detail = error.detail(),
                ?reason,
                "the copy-instead toast could not be shown"
            );
        }
        DeliveryReport {
            outcome: DeliveryOutcome::Copied,
            copy_reason: Some(reason),
            restore: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ports::fakes::{FakeClipboard, FakeForegroundApp, FakeNotifier, FakeTextInserter},
        types::{AppError, Permission, PortError},
    };

    const TEXT: &str = "Hello world. ";
    const PASTE_AND_KEEP: DeliveryPolicy = DeliveryPolicy {
        auto_paste: true,
        keep_on_clipboard: true,
    };
    const PASTE_AND_RESTORE: DeliveryPolicy = DeliveryPolicy {
        auto_paste: true,
        keep_on_clipboard: false,
    };
    const COPY_ONLY: DeliveryPolicy = DeliveryPolicy {
        auto_paste: false,
        keep_on_clipboard: true,
    };
    const PASTING_CAPS: InserterCaps = InserterCaps {
        can_target_elevated: false,
        uses_clipboard: true,
    };

    struct Rig {
        delivery: Delivery,
        clipboard: Arc<FakeClipboard>,
        inserter: Arc<FakeTextInserter>,
        notifier: Arc<FakeNotifier>,
    }

    fn rig_with(clipboard: FakeClipboard, inserter: FakeTextInserter) -> Rig {
        let clipboard = Arc::new(clipboard);
        let inserter = Arc::new(inserter);
        let notifier = Arc::new(FakeNotifier::default());
        let delivery = Delivery::new(DeliveryPorts {
            clipboard: Arc::clone(&clipboard) as _,
            inserter: Arc::clone(&inserter) as _,
            notifier: Arc::clone(&notifier) as _,
        });
        Rig {
            delivery,
            clipboard,
            inserter,
            notifier,
        }
    }

    fn rig() -> Rig {
        rig_with(
            FakeClipboard::with_text("before"),
            FakeTextInserter::default(),
        )
    }

    fn notepad() -> AppTarget {
        FakeForegroundApp::target("notepad.exe", false)
    }

    fn admin_terminal() -> AppTarget {
        FakeForegroundApp::target("wt.exe", true)
    }

    fn copied(reason: CopyReason) -> DeliveryReport {
        DeliveryReport {
            outcome: DeliveryOutcome::Copied,
            copy_reason: Some(reason),
            restore: None,
        }
    }

    #[test]
    fn plan_covers_every_policy_target_and_caps() {
        let notepad = notepad();
        let admin = admin_terminal();
        let reaches_elevated = InserterCaps {
            can_target_elevated: true,
            ..PASTING_CAPS
        };
        let cases = [
            (
                "",
                Some(&notepad),
                PASTE_AND_KEEP,
                PASTING_CAPS,
                DeliveryPlan::Nothing,
            ),
            (
                " \n",
                Some(&notepad),
                COPY_ONLY,
                PASTING_CAPS,
                DeliveryPlan::Nothing,
            ),
            (
                TEXT,
                Some(&notepad),
                PASTE_AND_KEEP,
                PASTING_CAPS,
                DeliveryPlan::Paste(notepad.clone()),
            ),
            (
                TEXT,
                Some(&notepad),
                PASTE_AND_RESTORE,
                PASTING_CAPS,
                DeliveryPlan::Paste(notepad.clone()),
            ),
            (
                TEXT,
                Some(&notepad),
                COPY_ONLY,
                PASTING_CAPS,
                DeliveryPlan::Copy(CopyReason::AutoPasteOff),
            ),
            (
                TEXT,
                None,
                PASTE_AND_KEEP,
                PASTING_CAPS,
                DeliveryPlan::Copy(CopyReason::NoTarget),
            ),
            (
                TEXT,
                None,
                COPY_ONLY,
                PASTING_CAPS,
                DeliveryPlan::Copy(CopyReason::AutoPasteOff),
            ),
            (
                TEXT,
                Some(&admin),
                PASTE_AND_KEEP,
                PASTING_CAPS,
                DeliveryPlan::Copy(CopyReason::ElevatedTarget),
            ),
            (
                TEXT,
                Some(&admin),
                COPY_ONLY,
                PASTING_CAPS,
                DeliveryPlan::Copy(CopyReason::AutoPasteOff),
            ),
            (
                TEXT,
                Some(&admin),
                PASTE_AND_KEEP,
                reaches_elevated,
                DeliveryPlan::Paste(admin.clone()),
            ),
        ];
        for (text, target, policy, caps, expected) in cases {
            assert_eq!(
                plan(text, target, policy, caps),
                expected,
                "{text:?} {target:?} {policy:?} {caps:?}"
            );
        }
    }

    #[test]
    fn auto_paste_on_writes_the_clipboard_excluded_then_pastes() {
        let rig = rig();
        let report = rig
            .delivery
            .deliver(TEXT, Some(&notepad()), PASTE_AND_KEEP)
            .unwrap();
        assert_eq!(
            report,
            DeliveryReport {
                outcome: DeliveryOutcome::Pasted,
                copy_reason: None,
                restore: None,
            }
        );
        assert_eq!(
            rig.clipboard.writes(),
            [(String::from(TEXT), ClipboardHistory::Exclude)]
        );
        assert_eq!(rig.inserter.insertions(), [(notepad(), String::from(TEXT))]);
        assert!(rig.notifier.toasts().is_empty());
    }

    #[test]
    fn auto_paste_off_copies_without_a_toast() {
        let rig = rig();
        let report = rig
            .delivery
            .deliver(TEXT, Some(&notepad()), COPY_ONLY)
            .unwrap();
        assert_eq!(report, copied(CopyReason::AutoPasteOff));
        assert_eq!(rig.clipboard.text().as_deref(), Some(TEXT));
        assert_eq!(
            rig.clipboard.last_history(),
            Some(ClipboardHistory::Exclude)
        );
        assert!(rig.inserter.insertions().is_empty());
        assert!(rig.notifier.toasts().is_empty());
    }

    #[test]
    fn an_elevated_target_is_copied_with_the_paste_blocked_toast() {
        let rig = rig();
        let report = rig
            .delivery
            .deliver(TEXT, Some(&admin_terminal()), PASTE_AND_KEEP)
            .unwrap();
        assert_eq!(report, copied(CopyReason::ElevatedTarget));
        assert!(
            rig.inserter.insertions().is_empty(),
            "the inserter is never asked to reach an elevated window"
        );
        assert_eq!(rig.clipboard.text().as_deref(), Some(TEXT));
        assert_eq!(rig.notifier.toasts(), [PASTE_BLOCKED_TOAST]);
        assert_eq!(
            format!(
                "{}. {}",
                PASTE_BLOCKED_TOAST.title, PASTE_BLOCKED_TOAST.body
            ),
            "Couldn't paste into that window. Copied instead. Press Ctrl+V."
        );
    }

    #[test]
    fn a_take_without_a_focused_window_is_copied_with_its_toast() {
        let rig = rig();
        let report = rig.delivery.deliver(TEXT, None, PASTE_AND_KEEP).unwrap();
        assert_eq!(report, copied(CopyReason::NoTarget));
        assert_eq!(rig.clipboard.text().as_deref(), Some(TEXT));
        assert_eq!(rig.notifier.toasts(), [NO_TARGET_TOAST]);
    }

    #[test]
    fn a_busy_clipboard_fails_delivery_without_pasting_or_toasting() {
        let busy = AppError::PermissionDenied {
            permission: Permission::Clipboard,
        };
        for policy in [PASTE_AND_KEEP, COPY_ONLY] {
            let rig = rig();
            rig.clipboard.hold(1);
            let result = rig.delivery.deliver(TEXT, Some(&notepad()), policy);
            assert_eq!(
                result.map_err(PortError::into_app_error),
                Err(busy.clone()),
                "{policy:?}"
            );
            assert!(rig.inserter.insertions().is_empty(), "{policy:?}");
            assert!(rig.notifier.toasts().is_empty(), "{policy:?}");
            assert_eq!(
                rig.clipboard.text().as_deref(),
                Some("before"),
                "{policy:?}"
            );
        }
    }

    #[test]
    fn a_failed_paste_leaves_the_text_copied_and_says_so() {
        let rig = rig();
        rig.inserter
            .fail_next(PortError::new(AppError::Internal).with_detail("window closed"));
        let report = rig
            .delivery
            .deliver(TEXT, Some(&notepad()), PASTE_AND_RESTORE)
            .unwrap();
        assert_eq!(report, copied(CopyReason::PasteFailed));
        assert_eq!(
            rig.clipboard.writes(),
            [(String::from(TEXT), ClipboardHistory::Exclude)],
            "written once; a failed paste keeps it even with keep_on_clipboard off"
        );
        assert_eq!(rig.notifier.toasts(), [PASTE_BLOCKED_TOAST]);
    }

    #[test]
    fn a_failed_toast_never_fails_delivery() {
        let rig = rig();
        rig.notifier.fail_next(AppError::Internal.into());
        let report = rig
            .delivery
            .deliver(TEXT, Some(&admin_terminal()), PASTE_AND_KEEP)
            .unwrap();
        assert_eq!(report, copied(CopyReason::ElevatedTarget));
        assert_eq!(rig.clipboard.text().as_deref(), Some(TEXT));
    }

    #[test]
    fn empty_text_touches_nothing() {
        let rig = rig();
        let report = rig
            .delivery
            .deliver("  ", Some(&notepad()), PASTE_AND_RESTORE)
            .unwrap();
        assert_eq!(report.outcome, DeliveryOutcome::NoSpeech);
        assert!(rig.clipboard.writes().is_empty());
        assert!(rig.inserter.insertions().is_empty());
        assert!(rig.notifier.toasts().is_empty());
    }

    #[test]
    fn an_inserter_that_types_needs_no_clipboard_until_it_fails() {
        let typing = InserterCaps {
            can_target_elevated: false,
            uses_clipboard: false,
        };
        let rig = rig_with(
            FakeClipboard::with_text("before"),
            FakeTextInserter::new(typing),
        );
        let report = rig
            .delivery
            .deliver(TEXT, Some(&notepad()), PASTE_AND_RESTORE)
            .unwrap();
        assert_eq!(report.outcome, DeliveryOutcome::Pasted);
        assert_eq!(report.restore, None, "the clipboard was never touched");
        assert!(rig.clipboard.writes().is_empty());

        rig.inserter.fail_next(AppError::Internal.into());
        let report = rig
            .delivery
            .deliver(TEXT, Some(&notepad()), PASTE_AND_KEEP)
            .unwrap();
        assert_eq!(report, copied(CopyReason::PasteFailed));
        assert_eq!(
            rig.clipboard.writes(),
            [(String::from(TEXT), ClipboardHistory::Exclude)]
        );
    }

    #[test]
    fn keep_on_clipboard_off_hands_back_the_previous_text() {
        let rig = rig();
        let report = rig
            .delivery
            .deliver(TEXT, Some(&notepad()), PASTE_AND_RESTORE)
            .unwrap();
        let restore = report.restore.unwrap();
        assert_eq!(restore.previous.as_deref(), Some("before"));
        assert_eq!(rig.clipboard.text().as_deref(), Some(TEXT));
        assert!(rig.delivery.restore_clipboard(&restore).unwrap());
        assert_eq!(rig.clipboard.text().as_deref(), Some("before"));
        assert_eq!(
            rig.clipboard.last_history(),
            Some(ClipboardHistory::Exclude)
        );
    }

    #[test]
    fn a_restore_leaves_a_newer_copy_alone() {
        let rig = rig();
        let restore = rig
            .delivery
            .deliver(TEXT, Some(&notepad()), PASTE_AND_RESTORE)
            .unwrap()
            .restore
            .unwrap();
        rig.clipboard
            .write_text("copied by the user meanwhile", ClipboardHistory::Include)
            .unwrap();
        assert!(!rig.delivery.restore_clipboard(&restore).unwrap());
        assert_eq!(
            rig.clipboard.text().as_deref(),
            Some("copied by the user meanwhile")
        );
    }

    #[test]
    fn a_restore_without_previous_text_clears_the_transcript() {
        for (clipboard, hold_the_read) in [
            (FakeClipboard::default(), false),
            (FakeClipboard::with_text("unreadable"), true),
        ] {
            let rig = rig_with(clipboard, FakeTextInserter::default());
            if hold_the_read {
                // Busy for exactly the read before the paste: the write and the paste still go through.
                rig.clipboard.hold(1);
            }
            let restore = rig
                .delivery
                .deliver(TEXT, Some(&notepad()), PASTE_AND_RESTORE)
                .unwrap()
                .restore
                .unwrap();
            assert_eq!(restore.previous, None);
            assert!(rig.delivery.restore_clipboard(&restore).unwrap());
            assert_eq!(rig.clipboard.text(), None);
        }
    }

    #[test]
    fn copy_writes_excluded() {
        let rig = rig();
        rig.delivery.copy("from history").unwrap();
        assert_eq!(
            rig.clipboard.writes(),
            [(String::from("from history"), ClipboardHistory::Exclude)]
        );
    }

    #[test]
    fn a_rehearsed_take_is_shown_in_echo_and_touches_nothing() {
        assert_eq!(
            keep_in_app(TEXT),
            DeliveryReport {
                outcome: DeliveryOutcome::Shown,
                copy_reason: None,
                restore: None,
            }
        );
        assert_eq!(keep_in_app("  \t ").outcome, DeliveryOutcome::NoSpeech);
    }
}
