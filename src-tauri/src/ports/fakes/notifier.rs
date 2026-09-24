/*!
 * SOURCE OF TRUTH KEYWORDS: FakeNotifier, fake toasts, recorded notifications, toast failure test
 * WHAT:  FakeNotifier: a Notifier that records every toast and can fail the next one.
 * WHY:   Tests assert which toast a flow shows (copy-only, device lost, recovered takes) and that a failed toast
 *        never fails the take.
 * WHERE: pipeline delivery, recovery and power tests.
 */

use std::sync::Mutex;

use super::lock;
use crate::{
    ports::Notifier,
    types::{PortError, PortResult, Toast},
};

#[derive(Default)]
struct NotifierLog {
    toasts: Vec<Toast>,
    next_error: Option<PortError>,
}

/// A recording notifier.
#[derive(Default)]
pub struct FakeNotifier {
    log: Mutex<NotifierLog>,
}

impl FakeNotifier {
    pub fn fail_next(&self, error: PortError) {
        lock(&self.log).next_error = Some(error);
    }

    pub fn toasts(&self) -> Vec<Toast> {
        lock(&self.log).toasts.clone()
    }
}

impl Notifier for FakeNotifier {
    fn toast(&self, toast: &Toast) -> PortResult<()> {
        let mut log = lock(&self.log);
        if let Some(error) = log.next_error.take() {
            return Err(error);
        }
        log.toasts.push(toast.clone());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{AppError, StaticStr, ToastKind};

    #[test]
    fn records_toasts_unless_told_to_fail() {
        const COPIED: Toast = Toast {
            kind: ToastKind::Info,
            title: StaticStr::new("Copied"),
            body: StaticStr::new("Press Ctrl+V to paste."),
        };
        let notifier = FakeNotifier::default();
        notifier.fail_next(AppError::Internal.into());
        assert!(notifier.toast(&COPIED).is_err());
        notifier.toast(&COPIED).unwrap();
        assert_eq!(notifier.toasts(), [COPIED]);
    }
}
