// SPDX-License-Identifier: GPL-3.0-only

#[cfg(feature = "backdrop")]
mod backdrop;
pub(crate) mod planner;
#[cfg(test)]
mod tests;

pub use crate::failure::{DrawFailure, FailedDraw};
pub use inventory;
pub(crate) use planner::EffectPlanner;
pub use planner::{EffectInput, QualityChange};

use crate::destination::Destination;
use crate::failure::Failures;
use crate::field::FieldRoom;
use crate::resources::PaintResources;
use crate::silhouette::clip_inside;
use lavashell_core::surface::{PerSurface, SurfaceId};
use lavashell_core::{scene::DrawCommand, shape::Shape};
use lavashell_effect_quality::{DrawId, EffectId, OfferedVariant, VariantId};
use skia_safe::{Canvas, Paint};
use std::collections::BTreeMap;
use std::collections::btree_map::Entry;

pub struct EffectRegistration {
    pub install: fn(&mut EffectRegistry, &FieldRoom) -> Result<(), crate::Error>,
}

inventory::collect!(EffectRegistration);

pub struct EffectRequest<'a> {
    pub surface: SurfaceId,
    pub id: DrawId,
    pub variant: VariantId,
    pub command: &'a DrawCommand,
    pub input: Option<&'a EffectInput>,
    pub access: SurfaceAccess,
    pub painter: ShapePainter<'a>,
}

#[derive(Clone, Copy)]
pub struct ShapePainter<'a> {
    resources: &'a PaintResources,
    failures: &'a Failures,
}

impl<'a> ShapePainter<'a> {
    pub(crate) fn new(resources: &'a PaintResources, failures: &'a Failures) -> Self {
        Self {
            resources,
            failures,
        }
    }

    pub(crate) fn resources(&self) -> &'a PaintResources {
        self.resources
    }

    pub fn fill(&self, canvas: &Canvas, shape: &Shape, paint: &Paint) -> Result<(), FailedDraw> {
        crate::effects::fill(canvas, self.resources, shape, paint)
    }

    pub fn clip_inside(&self, canvas: &Canvas, shape: &Shape) -> Result<(), FailedDraw> {
        clip_inside(canvas, self.resources, shape)
    }

    pub fn record(&self, failed: impl Into<FailedDraw>) {
        self.failures.record(failed);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceAccess {
    Unreadable,
    Current,
    Cacheable,
}

impl SurfaceAccess {
    pub(crate) fn new(destination: Destination, input: Option<&EffectInput>) -> Self {
        match destination {
            Destination::Layer => Self::Unreadable,
            Destination::Surface if input.is_some_and(|i| i.cache_allowed) => Self::Cacheable,
            Destination::Surface => Self::Current,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DrawTarget {
    pub pixels_per_dp: f64,
}

pub trait EffectBackend {
    fn variants(&self, command: &DrawCommand, target: DrawTarget) -> Vec<OfferedVariant>;

    fn render(&mut self, canvas: &Canvas, request: &EffectRequest<'_>);

    fn reads_destination(&self) -> bool {
        false
    }

    fn uses_command_footprint(&self) -> bool {
        !self.reads_destination()
    }

    fn forget_surface(&mut self, _surface: SurfaceId) {}
}

type Matcher = fn(&DrawCommand) -> bool;

struct Definition {
    accepts: Matcher,
    backend: Box<dyn EffectBackend>,
}

pub struct EffectRegistry {
    revision: u64,
    definitions: BTreeMap<EffectId, Definition>,
}

impl EffectRegistry {
    #[must_use]
    pub fn empty() -> Self {
        Self {
            revision: 0,
            definitions: BTreeMap::new(),
        }
    }

    pub fn new(room: &FieldRoom) -> Result<Self, crate::Error> {
        let mut registry = Self::empty();
        for registration in inventory::iter::<EffectRegistration> {
            (registration.install)(&mut registry, room)?;
        }
        Ok(registry)
    }

    pub fn register_effect(
        &mut self,
        effect: EffectId,
        accepts: Matcher,
        backend: impl EffectBackend + 'static,
    ) -> Result<(), crate::Error> {
        let Entry::Vacant(slot) = self.definitions.entry(effect) else {
            return Err(crate::Error::DuplicateEffect(effect));
        };
        let _registered_definition = slot.insert(Definition {
            accepts,
            backend: Box::new(backend),
        });
        self.revision += 1;
        Ok(())
    }

    #[must_use]
    pub fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub fn identify(&self, command: &DrawCommand) -> Option<EffectId> {
        if command.is_structural() {
            return None;
        }
        self.definitions
            .iter()
            .find(|(_, d)| (d.accepts)(command))
            .map(|(id, _)| *id)
    }

    #[must_use]
    pub fn variants(
        &self,
        effect: EffectId,
        command: &DrawCommand,
        target: DrawTarget,
    ) -> Vec<OfferedVariant> {
        (self.definitions.get(&effect))
            .map(|definition| definition.backend.variants(command, target))
            .unwrap_or_default()
    }

    pub(crate) fn reads_destination(&self, effect: EffectId) -> bool {
        (self.definitions.get(&effect))
            .is_some_and(|definition| definition.backend.reads_destination())
    }

    pub(crate) fn needs_full_damage(&self, effect: EffectId) -> bool {
        (self.definitions.get(&effect)).is_some_and(|definition| {
            let backend = &definition.backend;
            backend.reads_destination() && !backend.uses_command_footprint()
        })
    }

    pub fn render(&mut self, effect: EffectId, canvas: &Canvas, request: &EffectRequest<'_>) {
        if let Some(definition) = self.definitions.get_mut(&effect) {
            definition.backend.render(canvas, request);
        }
    }
}

impl PerSurface for EffectRegistry {
    fn forget_surface(&mut self, surface: SurfaceId) {
        for definition in self.definitions.values_mut() {
            definition.backend.forget_surface(surface);
        }
    }
}
