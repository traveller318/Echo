/*!
 * SOURCE OF TRUTH KEYWORDS: FakeOverlayWindow, OverlayCall, fake pill window, pointer over test, click-through test, overlay show hide test, drag test, scripted cursor
 * WHAT:  FakeOverlayWindow: an OverlayWindow that records every call, reports a scripted pointer position (over a
 *        button or not), a scripted cursor and primary button for drags, keeps the window corner its moves set, and
 *        whose next show can fail. ChannelSink-style waiting (`wait_for`) lets a test await calls
 *        a worker thread makes later.
 * WHY:   The pill presenter decides when to show, hide and take clicks on its own thread; tests prove those
 *        decisions without a window appearing on the test machine.
 * WHERE: pipeline/pill.rs tests; ipc::testing (default CommandCtx).
 */

use std::{
    sync::{Condvar, Mutex},
    time::{Duration, Instant},
};

use super::lock;
use crate::{
    ports::OverlayWindow,
    types::{OverlayPlacement, OverlayRect, PortError, PortResult, ScreenPoint},
};

/// One call the overlay received.
#[derive(Debug, Clone, PartialEq)]
pub enum OverlayCall {
    Show(OverlayPlacement),
    Hide,
    ClickThrough(bool),
    MoveTo(ScreenPoint),
}

#[derive(Default)]
struct OverlayState {
    calls: Vec<OverlayCall>,
    pointer_over: bool,
    next_error: Option<PortError>,
    origin: ScreenPoint,
    cursor: ScreenPoint,
    button_down: bool,
}

/// An overlay that records instead of drawing.
#[derive(Default)]
pub struct FakeOverlayWindow {
    state: Mutex<OverlayState>,
    changed: Condvar,
}

impl FakeOverlayWindow {
    /// Every successful call so far, in order.
    pub fn calls(&self) -> Vec<OverlayCall> {
        lock(&self.state).calls.clone()
    }

    /// Moves the pointer onto (`true`) or off (`false`) any clickable area.
    pub fn set_pointer_over(&self, over: bool) {
        lock(&self.state).pointer_over = over;
    }

    /// Puts the cursor at `cursor` with the primary button held (`down`) or not.
    pub fn set_cursor(&self, cursor: ScreenPoint, down: bool) {
        let mut state = lock(&self.state);
        state.cursor = cursor;
        state.button_down = down;
    }

    /// Makes the next show fail with `error` (and not be recorded).
    pub fn fail_next_show(&self, error: PortError) {
        lock(&self.state).next_error = Some(error);
    }

    /// Waits up to `timeout` until `done` holds for the calls so far; returns the calls either way.
    pub fn wait_for(
        &self,
        timeout: Duration,
        done: impl Fn(&[OverlayCall]) -> bool,
    ) -> Vec<OverlayCall> {
        let deadline = Instant::now() + timeout;
        let mut state = lock(&self.state);
        while !done(&state.calls) {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break;
            }
            state = self
                .changed
                .wait_timeout(state, left)
                .map_or_else(|poisoned| poisoned.into_inner().0, |(guard, _)| guard);
        }
        state.calls.clone()
    }

    fn record(&self, call: OverlayCall) {
        lock(&self.state).calls.push(call);
        self.changed.notify_all();
    }
}

impl OverlayWindow for FakeOverlayWindow {
    fn show(&self, placement: OverlayPlacement) -> PortResult<()> {
        if let Some(error) = lock(&self.state).next_error.take() {
            return Err(error);
        }
        if let OverlayPlacement::At(corner) = placement {
            lock(&self.state).origin = corner;
        }
        self.record(OverlayCall::Show(placement));
        Ok(())
    }

    fn hide(&self) -> PortResult<()> {
        self.record(OverlayCall::Hide);
        Ok(())
    }

    fn set_click_through(&self, through: bool) -> PortResult<()> {
        self.record(OverlayCall::ClickThrough(through));
        Ok(())
    }

    fn pointer_over(&self, areas: &[OverlayRect]) -> PortResult<bool> {
        Ok(!areas.is_empty() && lock(&self.state).pointer_over)
    }

    fn origin(&self) -> PortResult<ScreenPoint> {
        Ok(lock(&self.state).origin)
    }

    fn move_to(&self, corner: ScreenPoint) -> PortResult<()> {
        lock(&self.state).origin = corner;
        self.record(OverlayCall::MoveTo(corner));
        Ok(())
    }

    fn cursor(&self) -> PortResult<ScreenPoint> {
        Ok(lock(&self.state).cursor)
    }

    fn primary_button_down(&self) -> PortResult<bool> {
        Ok(lock(&self.state).button_down)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::AppError;

    #[test]
    fn records_calls_and_scripts_the_pointer() {
        let overlay = FakeOverlayWindow::default();
        overlay.fail_next_show(PortError::new(AppError::Internal));
        let primary = OverlayPlacement::BottomCentre(None);
        assert!(overlay.show(primary).is_err());
        overlay.show(primary).unwrap();
        overlay.set_click_through(false).unwrap();
        overlay.hide().unwrap();
        assert_eq!(
            overlay.calls(),
            [
                OverlayCall::Show(primary),
                OverlayCall::ClickThrough(false),
                OverlayCall::Hide
            ]
        );
        let stop = OverlayRect {
            x: 0,
            y: 0,
            width: 28,
            height: 28,
        };
        assert!(!overlay.pointer_over(&[stop]).unwrap());
        overlay.set_pointer_over(true);
        assert!(overlay.pointer_over(&[stop]).unwrap());
        assert!(
            !overlay.pointer_over(&[]).unwrap(),
            "no button, nothing to be over"
        );
        assert_eq!(
            overlay
                .wait_for(Duration::from_millis(1), |calls| calls.len() > 5)
                .len(),
            3
        );
    }

    #[test]
    fn moves_and_the_scripted_cursor_are_kept() {
        let overlay = FakeOverlayWindow::default();
        let corner = ScreenPoint { x: 40, y: 900 };
        overlay.move_to(corner).unwrap();
        assert_eq!(overlay.origin().unwrap(), corner);
        overlay.set_cursor(ScreenPoint { x: 5, y: 6 }, true);
        assert_eq!(overlay.cursor().unwrap(), ScreenPoint { x: 5, y: 6 });
        assert!(overlay.primary_button_down().unwrap());
        assert_eq!(overlay.calls(), [OverlayCall::MoveTo(corner)]);
    }
}
