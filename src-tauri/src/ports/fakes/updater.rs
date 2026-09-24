/*!
 * SOURCE OF TRUTH KEYWORDS: FakeUpdater, fake update check, disabled updater test, update available test
 * WHAT:  FakeUpdater: an Updater that is either disabled (the shipped behaviour) or offers a given version.
 * WHY:   The update commands and caps-driven UI hiding (02 §11) need both paths tested even though this build has
 *        no update source. `install` follows the port contract: nothing to install is `NotFound { update }`.
 * WHERE: `updates_check` / `updates_install` command tests; registry caps tests.
 */

use std::sync::Mutex;

use super::lock;
use crate::{
    ports::Updater,
    types::{AppError, BoxFuture, PortResult, ResourceKind, UpdateStatus, UpdaterCaps},
};

/// A scripted updater.
pub struct FakeUpdater {
    caps: UpdaterCaps,
    status: Mutex<UpdateStatus>,
    installs: Mutex<usize>,
}

impl FakeUpdater {
    /// Behaves like the shipped DisabledUpdater: unavailable, never checks anything.
    pub fn disabled() -> Self {
        Self {
            caps: UpdaterCaps { available: false },
            status: Mutex::new(UpdateStatus::NotConfigured),
            installs: Mutex::new(0),
        }
    }

    /// An updater that finds `version`.
    pub fn offering(version: &str) -> Self {
        Self {
            caps: UpdaterCaps { available: true },
            status: Mutex::new(UpdateStatus::Available {
                version: version.to_owned(),
                notes: None,
            }),
            installs: Mutex::new(0),
        }
    }

    pub fn installs(&self) -> usize {
        *lock(&self.installs)
    }
}

impl Updater for FakeUpdater {
    fn caps(&self) -> UpdaterCaps {
        self.caps
    }

    fn check(&self) -> BoxFuture<'_, PortResult<UpdateStatus>> {
        Box::pin(async move { Ok(lock(&self.status).clone()) })
    }

    fn install(&self) -> BoxFuture<'_, PortResult<()>> {
        Box::pin(async move {
            let mut status = lock(&self.status);
            match *status {
                UpdateStatus::Available { .. } => {
                    *status = UpdateStatus::UpToDate;
                    *lock(&self.installs) += 1;
                    Ok(())
                }
                UpdateStatus::NotConfigured | UpdateStatus::UpToDate => Err(AppError::NotFound {
                    resource: ResourceKind::Update,
                }
                .into()),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use std::task::Poll;

    use super::*;
    use crate::ports::fakes::poll_once;

    #[test]
    fn disabled_updater_reports_not_configured_and_installs_nothing() {
        let updater = FakeUpdater::disabled();
        assert!(!updater.caps().available);
        assert_eq!(
            poll_once(updater.check()),
            Poll::Ready(Ok(UpdateStatus::NotConfigured))
        );
        assert!(matches!(poll_once(updater.install()), Poll::Ready(Err(_))));
        assert_eq!(updater.installs(), 0);
    }

    #[test]
    fn an_offered_update_installs_once() {
        let updater = FakeUpdater::offering("0.2.0");
        assert!(matches!(
            poll_once(updater.check()),
            Poll::Ready(Ok(UpdateStatus::Available { .. }))
        ));
        assert_eq!(poll_once(updater.install()), Poll::Ready(Ok(())));
        assert_eq!(
            poll_once(updater.check()),
            Poll::Ready(Ok(UpdateStatus::UpToDate))
        );
        assert!(matches!(poll_once(updater.install()), Poll::Ready(Err(_))));
        assert_eq!(updater.installs(), 1);
    }
}
