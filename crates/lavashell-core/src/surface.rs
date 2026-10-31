// SPDX-License-Identifier: GPL-3.0-only
use crate::shape::{Polygon, RoundedRect};

/// Identifies a presentation surface, such as a window, across frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SurfaceId(pub u64);

impl SurfaceId {
    /// The surface of a frame drawn without a surface ID. A platform that numbers surfaces from zero gives the first
    /// surface this ID. Frames drawn without an ID then go to that first surface.
    pub const UNSPECIFIED: Self = Self(0);
}

/// The outline of a surface in surface-local dp, for one size of the surface.
#[derive(Clone, Debug, PartialEq)]
pub enum SurfaceOutline {
    Empty,
    /// Polygons filled together by the even-odd rule, so a polygon inside another one cuts a hole.
    Contours(Vec<Polygon>),
    /// A rounded rectangle whose corner radii are drawn fitted, each between zero and half the shorter side.
    Rounded(RoundedRect),
}
