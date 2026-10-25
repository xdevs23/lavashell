// SPDX-License-Identifier: GPL-3.0-only
/// Identifies a presentation surface, such as a window, across frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SurfaceId(pub u64);

impl SurfaceId {
    /// The surface of a frame drawn without a surface ID. A platform that numbers surfaces from zero gives the first
    /// surface this ID. Frames drawn without an ID then go to that first surface.
    pub const UNSPECIFIED: Self = Self(0);
}
