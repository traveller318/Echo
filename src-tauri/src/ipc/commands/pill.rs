/*!
 * SOURCE OF TRUTH KEYWORDS: pill commands, pill_set_hit_areas, pill_exited, pill_get_look, pill_drag, pill buttons click-through, pill exit animation, drag pill, PillLook read
 * WHAT:  The pill command group: `pill_set_hit_areas` tells Rust where the pill's clickable areas are (the only
 *        places it takes clicks); `pill_exited` says the pill's exit animation finished, so its window can be hidden;
 *        `pill_get_look` returns the pill settings the page renders (PillLook); `pill_drag` drags the pill with the
 *        cursor until the button is released and remembers where it was left.
 * WHY:   The pill window lets clicks through and never takes focus, so only its page knows where the buttons are after
 *        a morph and when the exit animation ends, while only Rust can move, hide and hit-test the window (05 W3,
 *        04 §4). The inputs go to the PillPresenter, which owns the window; none touches recording state. The
 *        areas are validated by the factory (PillHitAreas). The page starts a drag on a press of the pill surface
 *        while `pill.movable` is on; the presenter refuses one otherwise, and the command stays pending until the
 *        button is released, so one drag runs at a time (Exclusive) and its corner is stored once.
 * WHERE: Registered through `ipc::commands::catalog`; called by the pill page (src/pill: usePillHitAreas and the
 *        exit of its AnimatePresence) as `commands.pillSetHitAreas(…)` and `commands.pillExited()`; the pill page's
 *        usePillLook as `commands.pillGetLook()` and its surface as `commands.pillDrag()`.
 */

use crate::{
    ipc::{CommandCtx, factory::echo_command},
    pipeline::pill,
    registry,
    types::{AppError, PillHitAreas, PillLook, PortError},
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

echo_command! {
    /// The pill settings the pill page renders: when it shows, its style and whether it can be dragged.
    name: pill_get_look,
    output: PillLook,
    permission: None,
    reentrancy: Shared,
    handler: get_look,
}

echo_command! {
    /// Drags the pill with the cursor until the primary button is released, then remembers where it was left.
    name: pill_drag,
    output: (),
    permission: None,
    reentrancy: Exclusive("pill_drag"),
    handler: drag,
}

/// The PillLook of the settings in effect.
pub async fn get_look(ctx: &CommandCtx, (): ()) -> Result<PillLook, AppError> {
    Ok(registry::settings::pill_look(&ctx.settings()))
}

/// Waits for the presenter's drag to end and stores the corner it left the window at (nothing when it did not move).
pub async fn drag(ctx: &CommandCtx, (): ()) -> Result<(), PortError> {
    let Ok(Some(corner)) = ctx.pill().drag().await else {
        return Ok(());
    };
    pill::remember_position(ctx.shared_settings(), ctx.db(), ctx.events(), corner)
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
            CommandSpec, OverlayRect, PillVisibility, Reentrancy, SessionStateChanged,
            SessionStatus, SessionView, SettingValue, StaticStr,
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

    #[test]
    fn the_look_follows_the_settings_and_a_refused_drag_stores_nothing() {
        let settings = registry::settings::resolve([(
            registry::settings::keys::PILL_VISIBILITY,
            SettingValue::Enum(StaticStr::new(PillVisibility::Always.as_str())),
        )]);
        let harness = testing::harness(settings, FakePrivacyConsent::granted());
        let look = block_on(factory::run(
            &harness.ctx,
            &spec("pill_get_look"),
            (),
            get_look,
        ))
        .unwrap();
        assert_eq!(look.visibility, PillVisibility::Always);
        assert!(!look.movable);

        // Not movable (and never shown): the drag ends at once and nothing is stored.
        let drag_spec = CommandSpec {
            name: "pill_drag",
            permission: None,
            reentrancy: Reentrancy::Exclusive("pill_drag"),
        };
        block_on(factory::run(&harness.ctx, &drag_spec, (), drag)).unwrap();
        assert!(harness.events.events().is_empty());
        assert_eq!(
            registry::settings::pill_dragged_position(&harness.ctx.settings()),
            None
        );
    }
}
