/*!
 * SOURCE OF TRUTH KEYWORDS: pill commands, pill_set_hit_areas, pill_exited, pill buttons click-through, pill exit animation
 * WHAT:  The pill command group: `pill_set_hit_areas` tells Rust where the pill's buttons are (the only places it
 *        takes clicks); `pill_exited` says the pill's exit animation finished, so its window can be hidden.
 * WHY:   The pill window lets clicks through and never takes focus, so only its page knows where the buttons are after
 *        a morph and when the exit animation ends, while only Rust can move, hide and hit-test the window (05 W3,
 *        04 §4). Both inputs go to the PillPresenter, which owns the window; neither touches recording state. The
 *        areas are validated by the factory (PillHitAreas).
 * WHERE: Registered through `ipc::commands::catalog`; called by the pill page (src/pill: usePillHitAreas and the
 *        exit of its AnimatePresence) as `commands.pillSetHitAreas(…)` and `commands.pillExited()`.
 */

use crate::{
    ipc::{CommandCtx, factory::echo_command},
    types::{AppError, PillHitAreas},
};

echo_command! {
    /// Where the pill's buttons are now, in the page's CSS pixels; an empty list when it shows none.
    name: pill_set_hit_areas,
    input: PillHitAreas,
    output: (),
    permission: None,
    reentrancy: Shared,
    handler: set_hit_areas,
}

echo_command! {
    /// The pill finished its exit animation.
    name: pill_exited,
    output: (),
    permission: None,
    reentrancy: Shared,
    handler: exited,
}

/// Hands the button areas to the pill presenter.
pub async fn set_hit_areas(ctx: &CommandCtx, input: PillHitAreas) -> Result<(), AppError> {
    ctx.pill().set_hit_areas(input.areas);
    Ok(())
}

/// Lets the pill presenter hide the window.
pub async fn exited(ctx: &CommandCtx, (): ()) -> Result<(), AppError> {
    ctx.pill().exited();
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::{
        ipc::{factory, testing},
        ports::{
            EventSink,
            fakes::{FakePrivacyConsent, OverlayCall},
        },
        registry,
        types::{
            CommandSpec, OverlayRect, Reentrancy, SessionStateChanged, SessionStatus, SessionView,
        },
    };

    const fn spec(name: &'static str) -> CommandSpec {
        CommandSpec {
            name,
            permission: None,
            reentrancy: Reentrancy::Shared,
        }
    }

    fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(future)
    }

    #[test]
    fn invalid_areas_are_refused_and_the_exit_hides_a_shown_pill() {
        let harness = testing::harness(
            registry::settings::defaults(),
            FakePrivacyConsent::granted(),
        );
        let bad = PillHitAreas {
            areas: vec![OverlayRect {
                x: OverlayRect::MAX_COORD + 1,
                y: 0,
                width: 28,
                height: 28,
            }],
        };
        assert!(matches!(
            block_on(factory::run(
                &harness.ctx,
                &spec("pill_set_hit_areas"),
                bad,
                set_hit_areas
            )),
            Err(AppError::Validation { .. })
        ));

        // The pill follows the session events the context emits through its presenter.
        let pill = harness.ctx.pill().clone();
        pill.emit(
            SessionStateChanged(SessionView {
                status: SessionStatus::Recording,
                ..SessionView::IDLE
            })
            .into(),
        );
        pill.emit(SessionStateChanged(SessionView::IDLE).into());
        block_on(factory::run(&harness.ctx, &spec("pill_exited"), (), exited)).unwrap();
        let calls = harness.overlay.wait_for(Duration::from_secs(5), |calls| {
            calls.contains(&OverlayCall::Hide)
        });
        assert_eq!(calls.last(), Some(&OverlayCall::Hide));
        let ok = PillHitAreas { areas: Vec::new() };
        block_on(factory::run(
            &harness.ctx,
            &spec("pill_set_hit_areas"),
            ok,
            set_hit_areas,
        ))
        .unwrap();
    }
}
