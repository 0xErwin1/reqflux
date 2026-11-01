//! The `Chamfer` element: paints a cut shape behind or over a container's children.

use gpui::prelude::*;
use gpui::{App, Bounds, Hsla, PathBuilder, Pixels, Point, Window, canvas};

use super::geometry::{
    CutCorners, bottom_edge_polygon, cut_outline, edge_polygons, outside_polygons,
    snap_bounds_to_device, snap_length_to_device, snap_offset_to_device,
};

/// An edge painted just inside the outline, along the straight sides and the cuts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChamferEdge {
    /// Edge color.
    pub color: Hsla,
    /// Edge width, snapped to whole device pixels when painted.
    pub thickness: Pixels,
}

/// Paints a chamfered shape over the full box of its parent.
///
/// GPUI has no `clip-path`, so the shape is a canvas: place it as a child of a `.relative()`
/// container. It is absolutely positioned over the container, takes no part in layout, adds
/// no hitbox, and leaves the container's other children in the element tree. As the first
/// child it paints behind them; as the last child it paints over them, which is how an
/// [`outside_fill`](Self::outside_fill) masks content into the cut.
///
/// Paint order: fill, outside fill, edge, bottom edge. Bounds, cut and edge snap to the device pixel grid.
///
/// Not yet ported from DBFlux: hover and pressed fills with their transitions, and the focus
/// ring traced along the cut. They belong here, as builder options on this element, when the
/// first control needs them.
#[derive(IntoElement, Clone, Debug)]
pub struct Chamfer {
    cut: Pixels,
    corners: CutCorners,
    fill: Option<Hsla>,
    outside_fill: Option<Hsla>,
    edge: Option<ChamferEdge>,
    bottom_edge: Option<ChamferEdge>,
}

impl Chamfer {
    /// A shape with `cut` at the top-left and bottom-right corners that paints nothing until
    /// given a fill or an edge. Take `cut` from `tokens::cut`.
    pub fn new(cut: Pixels) -> Self {
        Self {
            cut,
            corners: CutCorners::default(),
            fill: None,
            outside_fill: None,
            edge: None,
            bottom_edge: None,
        }
    }

    /// Which corners are cut.
    pub fn corners(mut self, corners: CutCorners) -> Self {
        self.corners = corners;
        self
    }

    /// Fills the shape.
    pub fn fill(mut self, color: impl Into<Hsla>) -> Self {
        self.fill = Some(color.into());
        self
    }

    /// Fills the cut corners, outside the shape, with the color around it.
    pub fn outside_fill(mut self, color: impl Into<Hsla>) -> Self {
        self.outside_fill = Some(color.into());
        self
    }

    /// Paints an edge `thickness` wide just inside the outline, cuts included.
    pub fn edge(mut self, color: impl Into<Hsla>, thickness: Pixels) -> Self {
        self.edge = Some(ChamferEdge {
            color: color.into(),
            thickness,
        });
        self
    }

    /// Paints a line `thickness` wide just inside the bottom side, following a bottom cut:
    /// the edge under a keycap.
    pub fn bottom_edge(mut self, color: impl Into<Hsla>, thickness: Pixels) -> Self {
        self.bottom_edge = Some(ChamferEdge {
            color: color.into(),
            thickness,
        });
        self
    }

    /// Paints the shape into `bounds`.
    pub fn paint(&self, bounds: Bounds<Pixels>, window: &mut Window) {
        let scale_factor = window.scale_factor();
        let bounds = snap_bounds_to_device(bounds, scale_factor);
        let cut = snap_offset_to_device(self.cut, scale_factor);

        if let Some(fill) = self.fill {
            paint_polygon(&cut_outline(bounds, cut, self.corners), fill, window);
        }
        if let Some(outside) = self.outside_fill {
            for polygon in outside_polygons(bounds, cut, self.corners) {
                paint_polygon(&polygon, outside, window);
            }
        }
        if let Some(edge) = self.edge {
            let thickness = snap_length_to_device(edge.thickness, scale_factor);
            for polygon in edge_polygons(bounds, cut, self.corners, thickness) {
                paint_polygon(&polygon, edge.color, window);
            }
        }
        if let Some(edge) = self.bottom_edge {
            let thickness = snap_length_to_device(edge.thickness, scale_factor);
            let polygon = bottom_edge_polygon(bounds, cut, self.corners, thickness);
            paint_polygon(&polygon, edge.color, window);
        }
    }
}

impl RenderOnce for Chamfer {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        canvas(
            |_, _, _| {},
            move |bounds, (), window, _| self.paint(bounds, window),
        )
        .absolute()
        .inset_0()
    }
}

fn paint_polygon(points: &[Point<Pixels>], color: Hsla, window: &mut Window) {
    if points.len() < 3 || color.a <= 0.0 {
        return;
    }
    let Some((first, rest)) = points.split_first() else {
        return;
    };
    let mut builder = PathBuilder::fill();
    builder.move_to(*first);
    for point in rest {
        builder.line_to(*point);
    }
    builder.close();
    match builder.build() {
        Ok(path) => window.paint_path(path, color),
        Err(error) => log::warn!("chamfer path failed to build: {error}"),
    }
}
