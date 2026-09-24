/*!
 * SOURCE OF TRUTH KEYWORDS: OverlayRect, PillHitAreas, pill hit areas, click-through, overlay window, pill buttons, CSS pixels
 * WHAT:  OverlayRect (a rectangle on the pill's page, in CSS pixels from its top-left corner) and PillHitAreas (the
 *        rectangles of the pill's buttons, the only places the pill takes clicks), the input of `pill_set_hit_areas`.
 * WHY:   The pill is a transparent, non-activating window that must let clicks through everywhere except on its
 *        buttons (04 §4). Only the page knows where its buttons are after a morph, and only the overlay adapter
 *        knows the window's DPI and screen position, so the page reports rectangles in its own CSS pixels (1 CSS px
 *        = 1 DIP at the pill's fixed zoom) and the adapter converts them. Whole pixels are plenty for a hit test
 *        and keep the wire type a plain number (floats would be `number | null` in TS). The factory validates
 *        them: coordinates are bounded, and there are never more than a handful of buttons.
 * WHERE: Sent by src/pill (usePillHitArea) through `pill_set_hit_areas`; held by pipeline/pill.rs (PillPresenter);
 *        tested against the pointer by `OverlayWindow::pointer_over` (ports/overlay.rs).
 */

use serde::{Deserialize, Serialize};
use specta::Type;

/// A rectangle on the pill's page, in whole CSS pixels from its top-left corner.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type, garde::Validate,
)]
pub struct OverlayRect {
    #[garde(range(max = OverlayRect::MAX_COORD))]
    pub x: u32,
    #[garde(range(max = OverlayRect::MAX_COORD))]
    pub y: u32,
    #[garde(range(max = OverlayRect::MAX_COORD))]
    pub width: u32,
    #[garde(range(max = OverlayRect::MAX_COORD))]
    pub height: u32,
}

impl OverlayRect {
    /// Far beyond any overlay (the pill window is 360 × 88); anything larger is a bug in the page.
    pub const MAX_COORD: u32 = 10_000;

    /// True when (`x`, `y`) (CSS pixels, fractional) lies inside, edges included.
    pub fn contains(&self, x: f64, y: f64) -> bool {
        let (left, top) = (f64::from(self.x), f64::from(self.y));
        x >= left
            && x <= left + f64::from(self.width)
            && y >= top
            && y <= top + f64::from(self.height)
    }
}

/// The pill's clickable rectangles; everything else passes clicks through.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type, garde::Validate)]
pub struct PillHitAreas {
    #[garde(length(max = PillHitAreas::MAX_AREAS), dive)]
    pub areas: Vec<OverlayRect>,
}

impl PillHitAreas {
    /// The pill shows at most two buttons; a few more leaves room without inviting a flood.
    pub const MAX_AREAS: usize = 8;
}

#[cfg(test)]
mod tests {
    use garde::Validate;
    use serde_json::json;

    use super::*;

    fn rect(x: u32, y: u32, width: u32, height: u32) -> OverlayRect {
        OverlayRect {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn contains_includes_the_edges() {
        let stop = rect(10, 20, 28, 28);
        assert!(stop.contains(10.0, 20.0));
        assert!(stop.contains(38.0, 48.0));
        assert!(!stop.contains(38.5, 30.0));
        assert!(!stop.contains(9.9, 30.0));
    }

    #[test]
    fn areas_are_bounded() {
        let ok = PillHitAreas {
            areas: vec![rect(0, 0, 28, 28)],
        };
        assert!(ok.validate().is_ok());
        assert!(PillHitAreas { areas: vec![] }.validate().is_ok());
        let huge = PillHitAreas {
            areas: vec![rect(0, 0, 1_000_000, 28)],
        };
        assert!(huge.validate().is_err());
        let flood = PillHitAreas {
            areas: vec![rect(0, 0, 1, 1); PillHitAreas::MAX_AREAS + 1],
        };
        assert!(flood.validate().is_err());
    }

    #[test]
    fn serializes_as_plain_numbers() {
        assert_eq!(
            serde_json::to_value(PillHitAreas {
                areas: vec![rect(1, 2, 3, 4)]
            })
            .unwrap(),
            json!({ "areas": [{ "x": 1, "y": 2, "width": 3, "height": 4 }] })
        );
    }
}
