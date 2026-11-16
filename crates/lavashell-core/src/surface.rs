// SPDX-License-Identifier: GPL-3.0-only
use crate::geometry::Rect;
use crate::id_counter::{IdCounter, IdsExhausted};
use crate::panel::PanelGeometry;
use crate::shape::{Polygon, RoundedRect};
use crate::unique_id::UniqueId;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SurfaceId(u64);

impl SurfaceId {
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

#[cfg(any(test, feature = "test-support"))]
impl SurfaceId {
    #[must_use]
    pub const fn numbered(number: u64) -> Self {
        Self(number)
    }
}

#[derive(Default)]
pub struct SurfaceIds(IdCounter);

impl SurfaceIds {
    pub fn next_id(&mut self) -> Result<SurfaceId, IdsExhausted> {
        self.0.next_id().map(SurfaceId)
    }
}

pub trait PerSurface {
    fn forget_surface(&mut self, surface: SurfaceId);
}

impl<T: PerSurface> PerSurface for Option<T> {
    fn forget_surface(&mut self, surface: SurfaceId) {
        if let Some(state) = self {
            state.forget_surface(surface);
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum SurfaceOutline {
    Empty,
    Contours(Vec<Polygon>),
    Rounded(RoundedRect),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ConfigureToken(UniqueId);

impl Default for ConfigureToken {
    fn default() -> Self {
        Self(UniqueId::next())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SurfaceField {
    InputRegion,
    BlurRegion,
    Panel,
    Configure,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SurfaceChange {
    InputRegion(Option<Vec<Rect>>),
    BlurRegion(Option<Vec<Rect>>),
    Panel(PanelGeometry),
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

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SurfaceState {
    parts: BTreeMap<SurfaceField, SurfaceChange>,
}

impl SurfaceState {
    pub fn apply(&mut self, change: SurfaceChange) {
        let _replaced = self.parts.insert(change.field(), change);
    }

    #[must_use]
    pub fn contains(&self, change: &SurfaceChange) -> bool {
        self.parts.get(&change.field()) == Some(change)
    }

    #[must_use]
    pub fn get(&self, field: SurfaceField) -> Option<&SurfaceChange> {
        self.parts.get(&field)
    }

    pub fn changes(&self) -> impl Iterator<Item = SurfaceChange> {
        self.parts.values().cloned()
    }
}

#[cfg(test)]
mod tests;
