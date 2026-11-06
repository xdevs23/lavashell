// SPDX-License-Identifier: GPL-3.0-only
use crate::geometry::Rect;
use crate::panel::PanelGeometry;
use crate::shape::{Polygon, RoundedRect};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};

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

/// Identifies a configure event sent by the host for a surface. The surface state of the frame that applies the event
/// contains it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ConfigureToken(u64);

impl Default for ConfigureToken {
    /// A token that differs from every token made before in the process.
    fn default() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

/// A part of the state of a surface, set as a whole by a [`SurfaceChange`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SurfaceField {
    InputRegion,
    BlurRegion,
    Panel,
    Configure,
}

/// Sets one part of the state of a surface. Regions are in surface-local dp.
#[derive(Clone, Debug, PartialEq)]
pub enum SurfaceChange {
    /// The area in which the surface receives input, or `None` for the whole surface.
    InputRegion(Option<Vec<Rect>>),
    /// The area behind the surface, blurred by the host, or `None` without blur.
    BlurRegion(Option<Vec<Rect>>),
    Panel(PanelGeometry),
    /// The last configure event applied by the frame.
    Configure(ConfigureToken),
}

impl SurfaceChange {
    #[must_use]
    pub fn field(&self) -> SurfaceField {
        match self {
            Self::InputRegion(_) => SurfaceField::InputRegion,
            Self::BlurRegion(_) => SurfaceField::BlurRegion,
            Self::Panel(_) => SurfaceField::Panel,
            Self::Configure(_) => SurfaceField::Configure,
        }
    }
}

/// The state of a surface, applied by the host together with a frame of that surface. A part set by no change keeps
/// the default of the host.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SurfaceState {
    parts: BTreeMap<SurfaceField, SurfaceChange>,
}

impl SurfaceState {
    pub fn apply(&mut self, change: SurfaceChange) {
        let _replaced = self.parts.insert(change.field(), change);
    }

    /// Whether the part of the state set by `change` has the value of `change` already.
    #[must_use]
    pub fn contains(&self, change: &SurfaceChange) -> bool {
        self.parts.get(&change.field()) == Some(change)
    }

    /// The change that set `field` last, or `None` while the part keeps the default of the host.
    #[must_use]
    pub fn get(&self, field: SurfaceField) -> Option<&SurfaceChange> {
        self.parts.get(&field)
    }

    /// The change that set each part of this state, so applying them to a default state results in this state.
    pub fn changes(&self) -> impl Iterator<Item = SurfaceChange> {
        self.parts.values().cloned()
    }
}

#[cfg(test)]
mod tests;
