//! A disclosure chevron drawn as a stroked path, so it needs no icon font or asset.

use gpui::prelude::*;
use gpui::{App, Bounds, Hsla, PathBuilder, Pixels, Point, Window, canvas, div, point};

use crate::tokens::icon;

/// Where a chevron points.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChevronDirection {
    /// Points right: a collapsed disclosure.
    Right,
    /// Points down: an expanded disclosure.
    Down,
}

/// The three points of a chevron centered in `bounds`, inset so the stroke stays inside.
///
/// The chevron spans half the smaller side of `bounds`; its legs meet at the tip.
pub fn chevron_points(bounds: Bounds<Pixels>, direction: ChevronDirection) -> [Point<Pixels>; 3] {
    let side = bounds.size.width.min(bounds.size.height);
    let half = side / 4.0;
    let center = bounds.center();
    match direction {
        ChevronDirection::Right => [
            point(center.x - half / 2.0, center.y - half),
            point(center.x + half / 2.0, center.y),
            point(center.x - half / 2.0, center.y + half),
        ],
        ChevronDirection::Down => [
            point(center.x - half, center.y - half / 2.0),
            point(center.x, center.y + half / 2.0),
            point(center.x + half, center.y - half / 2.0),
        ],
    }
}

/// A small line chevron, `icon::SM` square, stroked `icon::STROKE` wide in `color`.
#[derive(IntoElement, Clone, Copy, Debug)]
pub struct Chevron {
    direction: ChevronDirection,
    color: Hsla,
}

impl Chevron {
    /// A chevron pointing in `direction`.
    pub fn new(direction: ChevronDirection, color: impl Into<Hsla>) -> Self {
        Self {
            direction,
            color: color.into(),
        }
    }
}

impl RenderOnce for Chevron {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        div().size(icon::SM).flex_none().child(
            canvas(
                |_, _, _| {},
                move |bounds, (), window, _| paint(bounds, self, window),
            )
            .size_full(),
        )
    }
}

fn paint(bounds: Bounds<Pixels>, chevron: Chevron, window: &mut Window) {
    let [start, tip, end] = chevron_points(bounds, chevron.direction);
    let mut builder = PathBuilder::stroke(icon::STROKE);
    builder.move_to(start);
    builder.line_to(tip);
    builder.line_to(end);
    match builder.build() {
        Ok(path) => window.paint_path(path, chevron.color),
        Err(error) => log::warn!("chevron path failed to build: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use gpui::{Bounds, point, px, size};

    use super::*;

    fn bounds() -> Bounds<Pixels> {
        Bounds::new(point(px(10.0), px(20.0)), size(px(12.0), px(12.0)))
    }

    #[test]
    fn a_collapsed_chevron_points_right() {
        assert_eq!(
            chevron_points(bounds(), ChevronDirection::Right),
            [
                point(px(14.5), px(23.0)),
                point(px(17.5), px(26.0)),
                point(px(14.5), px(29.0)),
            ]
        );
    }

    #[test]
    fn an_expanded_chevron_points_down() {
        assert_eq!(
            chevron_points(bounds(), ChevronDirection::Down),
            [
                point(px(13.0), px(24.5)),
                point(px(16.0), px(27.5)),
                point(px(19.0), px(24.5)),
            ]
        );
    }

    #[test]
    fn the_chevron_uses_the_smaller_side() {
        let wide = Bounds::new(point(px(0.0), px(0.0)), size(px(40.0), px(12.0)));
        let [start, tip, _] = chevron_points(wide, ChevronDirection::Right);
        assert_eq!(tip.x - start.x, px(3.0));
        assert_eq!(tip, point(px(21.5), px(6.0)));
    }
}
