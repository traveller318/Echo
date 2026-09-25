/*!
 * SOURCE OF TRUTH KEYWORDS: DisabledUpdater, no update source, updates not configured, UpdaterCaps available false, no network update
 * WHAT:  The Updater of this build: declares `available: false`, answers every check with `not_configured` and has
 *        nothing to install (`NotFound { update }`).
 * WHY:   Echo has no update endpoint (02 §11, no git or hosting), so it must never make an update network call; the
 *        caps hide `updates.auto_check` and the update buttons (registry CapsRequirement::UpdaterAvailable), and a
 *        caller that asks anyway gets a calm, typed answer instead of an error it cannot explain.
 * WHERE: Built by app/bootstrap into CommandCtx (settings availability now, the update commands later).
 */

use crate::{
    ports::Updater,
    types::{AppError, BoxFuture, PortResult, ResourceKind, UpdateStatus, UpdaterCaps},
};

/// An updater with no update source.
#[derive(Debug, Default, Clone, Copy)]
pub struct DisabledUpdater;

impl DisabledUpdater {
    pub const fn new() -> Self {
        Self
    }
}

impl Updater for DisabledUpdater {
    fn caps(&self) -> UpdaterCaps {
        UpdaterCaps { available: false }
    }

    fn check(&self) -> BoxFuture<'_, PortResult<UpdateStatus>> {
        Box::pin(async { Ok(UpdateStatus::NotConfigured) })
    }

    fn install(&self) -> BoxFuture<'_, PortResult<()>> {
        Box::pin(async {
            Err(AppError::NotFound {
                resource: ResourceKind::Update,
            }
            .into())
        })
    }
}

#[cfg(test)]
mod tests {
    use std::task::Poll;

    use super::*;
    use crate::ports::fakes::poll_once;

    #[test]
    fn it_is_unavailable_checks_nothing_and_installs_nothing() {
        let updater = DisabledUpdater::new();
        assert!(!updater.caps().available);
        assert_eq!(
            poll_once(updater.check()),
            Poll::Ready(Ok(UpdateStatus::NotConfigured))
        );
        let Poll::Ready(Err(error)) = poll_once(updater.install()) else {
            panic!("installing without an update must fail at once");
        };
        assert_eq!(
            error.into_app_error(),
            AppError::NotFound {
                resource: ResourceKind::Update
            }
        );
    }
}
