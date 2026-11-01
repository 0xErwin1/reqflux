//! Chamfered shapes: Bolt surfaces cut at 45 degrees instead of rounded.
//!
//! GPUI has no `clip-path`, so shapes are painted as polygons. [`geometry`] holds the pure
//! outline math; [`Chamfer`] paints it.

mod element;
pub mod geometry;

pub use element::{Chamfer, ChamferEdge};

pub use geometry::{
    CutCorners, bottom_edge_polygon, clamp_cut, cut_outline, edge_polygons, outside_polygons,
    snap_bounds_to_device, snap_length_to_device, snap_offset_to_device,
};
