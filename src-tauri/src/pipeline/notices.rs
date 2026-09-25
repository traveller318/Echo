/*!
 * SOURCE OF TRUTH KEYWORDS: NoticeBoard, show_once, one-time notice, hint shown once, hidden setting flag, SettingsChanged for hidden setting
 * WHAT:  NoticeBoard::show_once(notice): shows a registry OneTimeNotice as a toast the first time it is asked for,
 *        and sets the notice's hidden setting so it never shows again (announced as SettingsChanged).
 * WHY:   The check and the write happen inside `SharedSettings::update`, which runs one writer at a time, so two
 *        takes racing for the same hint can never both show it. The toast is shown even when the flag cannot be
 *        stored (a busy database): the user still gets the advice once now, and the failure is logged; the worst case
 *        is seeing it again after a restart. The flag goes through the registry's validation like any settings
 *        write, so a hidden setting obeys the same rules as a visible one, and through the one settings store, so
 *        the cached snapshot always equals the table (pipeline/settings_store.rs).
 * WHERE: Built by the session runner (from SessionConfig's settings, database, notifier and events); later
 *        onboarding and tray hints reuse it with their own registry/notices.rs entries.
 */

use std::sync::Arc;

use super::settings_store;
use crate::{
    ports::{EventSink, Notifier},
    registry,
    services::Db,
    types::{AppEvent, OneTimeNotice, PortError, SettingValue, SettingsChanged, SharedSettings},
};

/// What showing a notice once works through.
#[derive(Clone)]
pub struct NoticeBoard {
    pub settings: SharedSettings,
    pub db: Db,
    pub notifier: Arc<dyn Notifier>,
    pub events: Arc<dyn EventSink<AppEvent>>,
}

/// Why a notice was not stored.
enum Skip {
    AlreadyShown,
    Failed(PortError),
}

impl NoticeBoard {
    /// Shows `notice` unless it was shown before; true when it was shown now.
    pub fn show_once(&self, notice: &OneTimeNotice) -> bool {
        let shown = SettingValue::Bool(true);
        let stored = self.settings.update(|current| {
            if registry::settings::notice_shown(current, notice) {
                return Err(Skip::AlreadyShown);
            }
            registry::settings::validate(&notice.shown, &shown, current)
                .map_err(|error| Skip::Failed(error.into()))?;
            settings_store::store(&self.db, &notice.shown, Some(&shown)).map_err(Skip::Failed)
        });
        match stored {
            Ok(_) => self.events.emit(
                SettingsChanged {
                    key: notice.shown.clone(),
                    value: shown,
                }
                .into(),
            ),
            Err(Skip::AlreadyShown) => return false,
            Err(Skip::Failed(error)) => tracing::warn!(
                setting = %notice.shown,
                detail = error.detail(),
                "a one-time notice could not be remembered; it may show again after a restart"
            ),
        }
        if let Err(error) = self.notifier.toast(&notice.toast) {
            tracing::warn!(
                detail = error.detail(),
                "a one-time notice could not be shown"
            );
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ports::fakes::{FakeNotifier, RecordingSink},
        registry::{notices::BLUETOOTH_MIC_NOTICE, settings},
        services,
    };

    struct Rig {
        board: NoticeBoard,
        notifier: Arc<FakeNotifier>,
        events: Arc<RecordingSink<AppEvent>>,
    }

    fn rig() -> Rig {
        let notifier = Arc::new(FakeNotifier::default());
        let events = Arc::new(RecordingSink::default());
        Rig {
            board: NoticeBoard {
                settings: SharedSettings::new(settings::defaults()),
                db: Db::open_in_memory().unwrap(),
                notifier: Arc::clone(&notifier) as _,
                events: Arc::clone(&events) as _,
            },
            notifier,
            events,
        }
    }

    #[test]
    fn a_notice_is_shown_and_remembered_once() {
        let rig = rig();
        assert!(rig.board.show_once(&BLUETOOTH_MIC_NOTICE));
        assert!(!rig.board.show_once(&BLUETOOTH_MIC_NOTICE));

        assert_eq!(
            rig.notifier.toasts(),
            std::slice::from_ref(&BLUETOOTH_MIC_NOTICE.toast)
        );
        assert!(settings::notice_shown(
            &rig.board.settings.current(),
            &BLUETOOTH_MIC_NOTICE
        ));
        assert_eq!(
            services::settings::get::all(&rig.board.db).unwrap(),
            [(BLUETOOTH_MIC_NOTICE.shown.clone(), SettingValue::Bool(true))]
        );
        assert_eq!(
            rig.events.events(),
            [AppEvent::SettingsChanged(SettingsChanged {
                key: BLUETOOTH_MIC_NOTICE.shown.clone(),
                value: SettingValue::Bool(true),
            })]
        );
    }

    #[test]
    fn a_notice_already_stored_as_shown_stays_quiet() {
        let rig = rig();
        rig.board.settings.replace(settings::resolve([(
            BLUETOOTH_MIC_NOTICE.shown.clone(),
            SettingValue::Bool(true),
        )]));
        assert!(!rig.board.show_once(&BLUETOOTH_MIC_NOTICE));
        assert!(rig.notifier.toasts().is_empty());
        assert!(rig.events.events().is_empty());
    }
}
