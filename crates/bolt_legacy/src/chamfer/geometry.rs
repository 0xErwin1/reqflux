//! Pure geometry of chamfered shapes: the cut outline, the edge along it, the corners outside
//! it, and device-pixel snapping.
//!
//! Every function works in the coordinate space of the `bounds` it receives and returns
//! nothing for empty or negative bounds, so callers paint without checks.

use gpui::{Bounds, Pixels, Point, Size, point, size};

/// Which corners of a rectangle are cut at 45 degrees.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CutCorners {
    /// Top-left and bottom-right cut, the other two square: islands, cards, buttons.
    #[default]
    TopLeftBottomRight,
    /// All four corners cut.
    All,
}

impl CutCorners {
    /// Whether the top-left, top-right, bottom-right and bottom-left corners are cut.
    fn cut_mask(self) -> [bool; 4] {
        match self {
            Self::TopLeftBottomRight => [true, false, true, false],
            Self::All => [true, true, true, true],
        }
    }
}

/// Clamps `cut` to between zero and half of the smaller side of `size`, and to zero for an
/// empty size, so the outline never crosses itself.
pub fn clamp_cut(size: Size<Pixels>, cut: Pixels) -> Pixels {
    let width = f32::from(size.width);
    let height = f32::from(size.height);
    if width <= 0.0 || height <= 0.0 {
        return Pixels::ZERO;
    }
    Pixels::from(f32::from(cut).clamp(0.0, width.min(height) / 2.0))
}

/// The outline of the cut shape, clockwise from the top-left corner, without repeated
/// vertices: a zero cut yields the plain rectangle.
pub fn cut_outline(bounds: Bounds<Pixels>, cut: Pixels, corners: CutCorners) -> Vec<Point<Pixels>> {
    let Some(rect) = Rect::of(bounds) else {
        return Vec::new();
    };
    let cut = f32::from(clamp_cut(bounds.size, cut));
    to_pixels(&dedup(rect.outline(cut, corners).to_vec()))
}

/// Polygons that paint an edge `thickness` wide just inside the outline, along the straight
/// sides and the diagonals alike: one per side of the outline, mitered so they meet without
/// overlapping, which keeps a translucent edge uniform. Bounds too small to hold the edge
/// yield the whole shape as one polygon.
pub fn edge_polygons(
    bounds: Bounds<Pixels>,
    cut: Pixels,
    corners: CutCorners,
    thickness: Pixels,
) -> Vec<Vec<Point<Pixels>>> {
    let thickness = f32::from(thickness);
    let Some(rect) = Rect::of(bounds) else {
        return Vec::new();
    };
    if thickness <= 0.0 {
        return Vec::new();
    }
    let cut = f32::from(clamp_cut(bounds.size, cut));
    let Some(inner) = rect.inset(thickness) else {
        return vec![to_pixels(&dedup(rect.outline(cut, corners).to_vec()))];
    };
    // Moving a 45 degree diagonal inward by `thickness` shortens its legs by
    // `thickness * (2 - sqrt 2)` on the inset rectangle.
    let inner_cut = (cut - thickness * (2.0 - std::f32::consts::SQRT_2)).max(0.0);

    let outer = rect.outline(cut, corners);
    let inner = inner.outline(inner_cut, corners);
    let next = (1..outer.len()).chain(std::iter::once(0));
    outer
        .iter()
        .zip(inner.iter())
        .zip(next.filter_map(|index| outer.get(index).zip(inner.get(index))))
        .map(|((&a, &a_in), (&b, &b_in))| vec![a, b, b_in, a_in])
        .filter(|quad| area(quad) > f32::EPSILON)
        .map(|quad| to_pixels(&dedup(quad)))
        .collect()
}

/// The part of the shape within `thickness` of its bottom side, following a bottom cut
/// diagonally like an inset bottom line clipped by the shape: the keycap's bottom edge.
pub fn bottom_edge_polygon(
    bounds: Bounds<Pixels>,
    cut: Pixels,
    corners: CutCorners,
    thickness: Pixels,
) -> Vec<Point<Pixels>> {
    let thickness = f32::from(thickness);
    let Some(rect) = Rect::of(bounds) else {
        return Vec::new();
    };
    if thickness <= 0.0 {
        return Vec::new();
    }
    let cut = f32::from(clamp_cut(bounds.size, cut));
    let band_top = (rect.bottom - thickness).max(rect.top);
    let outline = dedup(rect.outline(cut, corners).to_vec());
    to_pixels(&dedup(clip_below(&outline, band_top)))
}

/// The part of the convex `polygon` at or below `y`, keeping the vertex order.
fn clip_below(polygon: &[Point<f32>], y: f32) -> Vec<Point<f32>> {
    let next = polygon.iter().cycle().skip(1);
    let mut clipped = Vec::with_capacity(polygon.len() + 2);
    for (&from, &to) in polygon.iter().zip(next) {
        let from_inside = from.y >= y;
        let to_inside = to.y >= y;
        if from_inside {
            clipped.push(from);
        }
        if from_inside != to_inside {
            let t = (y - from.y) / (to.y - from.y);
            clipped.push(point(from.x + (to.x - from.x) * t, y));
        }
    }
    clipped
}

/// The corners of `bounds` outside the shape, one right triangle per cut corner. Painted in
/// the color around the shape, they mask content so it reads as cut.
pub fn outside_polygons(
    bounds: Bounds<Pixels>,
    cut: Pixels,
    corners: CutCorners,
) -> Vec<Vec<Point<Pixels>>> {
    let Some(rect) = Rect::of(bounds) else {
        return Vec::new();
    };
    let cut = f32::from(clamp_cut(bounds.size, cut));
    if cut <= 0.0 {
        return Vec::new();
    }
    let Rect {
        left,
        top,
        right,
        bottom,
    } = rect;
    let triangles = [
        [
            point(left, top),
            point(left + cut, top),
            point(left, top + cut),
        ],
        [
            point(right, top),
            point(right, top + cut),
            point(right - cut, top),
        ],
        [
            point(right, bottom),
            point(right - cut, bottom),
            point(right, bottom - cut),
        ],
        [
            point(left, bottom),
            point(left, bottom - cut),
            point(left + cut, bottom),
        ],
    ];
    triangles
        .iter()
        .zip(corners.cut_mask())
        .filter(|(_, is_cut)| *is_cut)
        .map(|(triangle, _)| to_pixels(triangle))
        .collect()
}

/// Rounds each side of `bounds` to the nearest device pixel, so straight edges land on whole
/// device pixels. A non-positive scale factor returns `bounds` unchanged.
pub fn snap_bounds_to_device(bounds: Bounds<Pixels>, scale_factor: f32) -> Bounds<Pixels> {
    let Some(rect) = Rect::of(bounds).filter(|_| scale_factor > 0.0) else {
        return bounds;
    };
    let snap = |value: f32| (value * scale_factor).round() / scale_factor;
    let left = snap(rect.left);
    let top = snap(rect.top);
    let right = snap(rect.right).max(left);
    let bottom = snap(rect.bottom).max(top);
    Bounds::new(
        point(Pixels::from(left), Pixels::from(top)),
        size(Pixels::from(right - left), Pixels::from(bottom - top)),
    )
}

/// Rounds an offset, such as a cut, to the nearest device pixel.
pub fn snap_offset_to_device(offset: Pixels, scale_factor: f32) -> Pixels {
    if scale_factor <= 0.0 {
        return offset;
    }
    Pixels::from((f32::from(offset) * scale_factor).round() / scale_factor)
}

/// Rounds a positive length to whole device pixels, never below one, so a hairline stays
/// crisp at fractional scale factors. Non-positive lengths become zero.
pub fn snap_length_to_device(length: Pixels, scale_factor: f32) -> Pixels {
    let length = f32::from(length);
    if length <= 0.0 {
        return Pixels::ZERO;
    }
    if scale_factor <= 0.0 {
        return Pixels::from(length);
    }
    Pixels::from((length * scale_factor).round().max(1.0) / scale_factor)
}

/// The sides of a non-empty rectangle, in logical pixels.
#[derive(Clone, Copy, Debug)]
struct Rect {
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
}

impl Rect {
    fn of(bounds: Bounds<Pixels>) -> Option<Self> {
        let left = f32::from(bounds.origin.x);
        let top = f32::from(bounds.origin.y);
        let rect = Self {
            left,
            top,
            right: left + f32::from(bounds.size.width),
            bottom: top + f32::from(bounds.size.height),
        };
        (rect.right > rect.left && rect.bottom > rect.top).then_some(rect)
    }

    fn inset(self, by: f32) -> Option<Self> {
        let rect = Self {
            left: self.left + by,
            top: self.top + by,
            right: self.right - by,
            bottom: self.bottom - by,
        };
        (rect.right > rect.left && rect.bottom > rect.top).then_some(rect)
    }

    /// Two vertices per corner, clockwise from the top-left; an uncut corner repeats its
    /// vertex, so outlines of any corner set pair up side by side.
    fn outline(self, cut: f32, corners: CutCorners) -> [Point<f32>; 8] {
        let [top_left, top_right, bottom_right, bottom_left] = corners
            .cut_mask()
            .map(|is_cut| if is_cut { cut } else { 0.0 });
        let Self {
            left,
            top,
            right,
            bottom,
        } = self;
        [
            point(left, top + top_left),
            point(left + top_left, top),
            point(right - top_right, top),
            point(right, top + top_right),
            point(right, bottom - bottom_right),
            point(right - bottom_right, bottom),
            point(left + bottom_left, bottom),
            point(left, bottom - bottom_left),
        ]
    }
}

fn dedup(mut points: Vec<Point<f32>>) -> Vec<Point<f32>> {
    points.dedup();
    while points.len() > 1 && points.first() == points.last() {
        points.pop();
    }
    points
}

fn area(polygon: &[Point<f32>]) -> f32 {
    let twice: f32 = polygon
        .iter()
        .zip(polygon.iter().cycle().skip(1))
        .map(|(a, b)| a.x * b.y - b.x * a.y)
        .sum();
    twice.abs() / 2.0
}

fn to_pixels(points: &[Point<f32>]) -> Vec<Point<Pixels>> {
    points
        .iter()
        .map(|p| point(Pixels::from(p.x), Pixels::from(p.y)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::px;

    fn rect(x: f32, y: f32, width: f32, height: f32) -> Bounds<Pixels> {
        Bounds::new(point(px(x), px(y)), size(px(width), px(height)))
    }

    fn raw(points: &[Point<Pixels>]) -> Vec<(f32, f32)> {
        points
            .iter()
            .map(|p| (f32::from(p.x), f32::from(p.y)))
            .collect()
    }

    fn assert_close(actual: &[Point<Pixels>], expected: &[(f32, f32)]) {
        let actual = raw(actual);
        assert_eq!(actual.len(), expected.len(), "{actual:?}");
        for (a, e) in actual.iter().zip(expected) {
            assert!(
                (a.0 - e.0).abs() < 1e-4 && (a.1 - e.1).abs() < 1e-4,
                "{actual:?} != {expected:?}"
            );
        }
    }

    #[test]
    fn bottom_edge_is_the_band_of_the_shape_along_the_bottom_side() {
        let band = bottom_edge_polygon(
            rect(0.0, 0.0, 40.0, 20.0),
            px(4.0),
            CutCorners::default(),
            px(1.0),
        );
        assert_close(
            &band,
            &[(37.0, 19.0), (36.0, 20.0), (0.0, 20.0), (0.0, 19.0)],
        );

        let all = bottom_edge_polygon(
            rect(0.0, 0.0, 40.0, 20.0),
            px(4.0),
            CutCorners::All,
            px(1.0),
        );
        assert_close(
            &all,
            &[(37.0, 19.0), (36.0, 20.0), (4.0, 20.0), (3.0, 19.0)],
        );
    }

    #[test]
    fn bottom_edge_of_empty_bounds_or_no_thickness_is_empty() {
        let empty = rect(0.0, 0.0, 0.0, 20.0);
        assert!(bottom_edge_polygon(empty, px(4.0), CutCorners::default(), px(1.0)).is_empty());
        let shape = rect(0.0, 0.0, 40.0, 20.0);
        assert!(bottom_edge_polygon(shape, px(4.0), CutCorners::default(), px(0.0)).is_empty());
    }

    #[test]
    fn island_outline_cuts_top_left_and_bottom_right() {
        let outline = cut_outline(
            rect(10.0, 20.0, 100.0, 50.0),
            px(14.0),
            CutCorners::default(),
        );
        assert_eq!(
            raw(&outline),
            [
                (10.0, 34.0),
                (24.0, 20.0),
                (110.0, 20.0),
                (110.0, 56.0),
                (96.0, 70.0),
                (10.0, 70.0),
            ]
        );
    }

    #[test]
    fn all_corners_outline_has_eight_vertices() {
        let outline = cut_outline(rect(0.0, 0.0, 40.0, 20.0), px(6.0), CutCorners::All);
        assert_eq!(
            raw(&outline),
            [
                (0.0, 6.0),
                (6.0, 0.0),
                (34.0, 0.0),
                (40.0, 6.0),
                (40.0, 14.0),
                (34.0, 20.0),
                (6.0, 20.0),
                (0.0, 14.0),
            ]
        );
    }

    #[test]
    fn cut_is_clamped_to_half_the_smaller_side() {
        assert_eq!(clamp_cut(size(px(10.0), px(6.0)), px(14.0)), px(3.0));
        assert_eq!(clamp_cut(size(px(10.0), px(6.0)), px(-2.0)), px(0.0));
        assert_eq!(clamp_cut(size(px(0.0), px(6.0)), px(14.0)), px(0.0));
        let outline = cut_outline(rect(0.0, 0.0, 10.0, 6.0), px(14.0), CutCorners::default());
        assert_eq!(
            raw(&outline),
            [
                (0.0, 3.0),
                (3.0, 0.0),
                (10.0, 0.0),
                (10.0, 3.0),
                (7.0, 6.0),
                (0.0, 6.0)
            ]
        );
    }

    #[test]
    fn zero_cut_is_the_plain_rectangle_and_empty_bounds_yield_nothing() {
        let outline = cut_outline(rect(0.0, 0.0, 8.0, 4.0), px(0.0), CutCorners::All);
        assert_eq!(
            raw(&outline),
            [(0.0, 0.0), (8.0, 0.0), (8.0, 4.0), (0.0, 4.0)]
        );
        let empty = rect(0.0, 0.0, 0.0, 4.0);
        assert!(cut_outline(empty, px(4.0), CutCorners::All).is_empty());
        assert!(edge_polygons(empty, px(4.0), CutCorners::All, px(1.0)).is_empty());
        assert!(outside_polygons(empty, px(4.0), CutCorners::All).is_empty());
    }

    #[test]
    fn island_edge_has_one_band_per_side_including_the_diagonals() {
        let bounds = rect(0.0, 0.0, 100.0, 50.0);
        let edges = edge_polygons(bounds, px(14.0), CutCorners::default(), px(1.0));
        assert_eq!(edges.len(), 6);
        // Inner cut: 14 - (2 - sqrt 2).
        let inner = 14.0 - (2.0 - std::f32::consts::SQRT_2);
        let mut edges = edges.into_iter();
        let Some(diagonal) = edges.next() else {
            unreachable!()
        };
        assert_close(
            &diagonal,
            &[
                (0.0, 14.0),
                (14.0, 0.0),
                (1.0 + inner, 1.0),
                (1.0, 1.0 + inner),
            ],
        );
        let Some(top) = edges.next() else {
            unreachable!()
        };
        assert_close(
            &top,
            &[(14.0, 0.0), (100.0, 0.0), (99.0, 1.0), (1.0 + inner, 1.0)],
        );
        let Some(right) = edges.next() else {
            unreachable!()
        };
        assert_close(
            &right,
            &[
                (100.0, 0.0),
                (100.0, 36.0),
                (99.0, 50.0 - 1.0 - inner),
                (99.0, 1.0),
            ],
        );
    }

    #[test]
    fn edge_counts_follow_the_corner_set() {
        let bounds = rect(0.0, 0.0, 60.0, 30.0);
        assert_eq!(
            edge_polygons(bounds, px(6.0), CutCorners::All, px(1.0)).len(),
            8
        );
        assert_eq!(
            edge_polygons(bounds, px(0.0), CutCorners::All, px(1.0)).len(),
            4
        );
        assert!(edge_polygons(bounds, px(6.0), CutCorners::All, px(0.0)).is_empty());
        // Too small for the edge: the whole shape is the edge.
        let tiny = rect(0.0, 0.0, 2.0, 1.5);
        assert_eq!(
            edge_polygons(tiny, px(6.0), CutCorners::All, px(1.0)).len(),
            1
        );
    }

    #[test]
    fn outside_polygons_cover_the_cut_corners() {
        let bounds = rect(0.0, 0.0, 100.0, 50.0);
        let triangles = outside_polygons(bounds, px(14.0), CutCorners::default());
        let triangles: Vec<_> = triangles.iter().map(|t| raw(t)).collect();
        assert_eq!(
            triangles,
            [
                vec![(0.0, 0.0), (14.0, 0.0), (0.0, 14.0)],
                vec![(100.0, 50.0), (86.0, 50.0), (100.0, 36.0)],
            ]
        );
        assert_eq!(outside_polygons(bounds, px(6.0), CutCorners::All).len(), 4);
        assert!(outside_polygons(bounds, px(0.0), CutCorners::All).is_empty());
    }

    #[test]
    fn snapping_rounds_to_device_pixels_and_keeps_one_device_pixel() {
        let snapped = snap_bounds_to_device(rect(0.3, 0.6, 10.4, 10.0), 2.0);
        assert_eq!(snapped, rect(0.5, 0.5, 10.0, 10.0));
        assert_eq!(
            snap_bounds_to_device(rect(0.3, 0.6, 1.0, 1.0), 0.0),
            rect(0.3, 0.6, 1.0, 1.0)
        );
        assert_eq!(snap_offset_to_device(px(14.3), 2.0), px(14.5));
        assert_eq!(snap_length_to_device(px(1.0), 1.5), px(1.0 / 1.5 * 2.0));
        assert_eq!(snap_length_to_device(px(0.2), 1.0), px(1.0));
        assert_eq!(snap_length_to_device(px(0.0), 2.0), px(0.0));
    }
}
