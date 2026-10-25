// SPDX-License-Identifier: GPL-3.0-only
//! The registry of effects that draw certain draw commands in place of the painter, such as the backdrop blur. An
//! `EffectRegistration` submitted through `inventory` installs an effect with a function that matches its commands
//! and a backend that lists its quality variants and draws them. The registry uses no built-in effect identifier or
//! quality level.

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
use lavashell_core::surface::SurfaceId;
use lavashell_core::{scene::DrawCommand, shape::Shape};
use lavashell_effect_quality::{DrawId, EffectId, VariantId};
use skia_safe::{Canvas, Paint};
use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::sync::atomic::{AtomicU64, Ordering};

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

/// Paints shapes the way the painter paints them and records failures in the painter.
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

    /// # Errors
    /// Returns the failure that prevented the shape from being filled.
    pub fn fill(&self, canvas: &Canvas, shape: &Shape, paint: &Paint) -> Result<(), FailedDraw> {
        crate::effects::fill(canvas, self.resources, shape, paint)
    }

    /// # Errors
    /// Returns the failure that prevented the canvas from being clipped to the shape.
    pub fn clip_inside(&self, canvas: &Canvas, shape: &Shape) -> Result<(), FailedDraw> {
        clip_inside(canvas, self.resources, shape)
    }

    pub fn record(&self, failed: impl Into<FailedDraw>) {
        self.failures.record(failed);
    }
}

/// What an effect may read from the surface below its canvas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceAccess {
    /// The effect draws into a layer above the surface, such as an opacity group, a content filter or the
    /// layer for blending in encoded sRGB. The surface below does not contain the pixels covered by the effect.
    Unreadable,
    /// The surface contains what the effect covers in this frame.
    Current,
    /// The surface contains what the effect covers. The effect may store what it reads and reuse it in later
    /// frames.
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

/// The space into which an effect draws, described by its pixel density.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DrawTarget {
    /// Device pixels per dp of the space the command is drawn in.
    pub pixels_per_dp: f64,
}

pub trait EffectBackend {
    /// The variants `command` can be drawn with on `target`, the best first.
    fn variants(&self, command: &DrawCommand, target: DrawTarget) -> Vec<VariantId>;

    fn render(&mut self, canvas: &Canvas, request: &EffectRequest<'_>);

    /// Whether the effect reads the pixels painted below it, as a backdrop blur does. An opacity group around
    /// such an effect starts its layer with the pixels of the parent, so that the effect can read them while the
    /// group fades.
    fn reads_destination(&self) -> bool {
        false
    }

    /// Whether every pixel read by the effect lies inside the footprint of its draw command. When an effect that
    /// reads the pixels below it returns `false`, the pixels it reads are unknown, and a partial repaint treats
    /// the whole surface as changed.
    fn uses_command_footprint(&self) -> bool {
        !self.reads_destination()
    }

    fn forget_surface(&mut self, _surface: SurfaceId) {}
}

type Matcher = fn(&DrawCommand) -> bool;

fn next_revision() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

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
            revision: next_revision(),
            definitions: BTreeMap::new(),
        }
    }

    /// Installs every registered effect. An effect lays out the uniforms of its shape fields in `room`.
    ///
    /// # Errors
    /// Fails when an effect cannot build what it draws with, such as its shader programs.
    pub fn new(room: &FieldRoom) -> Result<Self, crate::Error> {
        let mut registry = Self::empty();
        for registration in inventory::iter::<EffectRegistration> {
            (registration.install)(&mut registry, room)?;
        }
        Ok(registry)
    }

    /// # Errors
    /// Fails when `effect` is registered already.
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
        self.revision = next_revision();
        Ok(())
    }

    /// A number that differs between any two registries. Each registration changes it.
    #[must_use]
    pub fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub fn identify(&self, command: &DrawCommand) -> Option<EffectId> {
        // An effect replaces only a command that draws. The painter handles the commands that save, restore, fade,
        // transform, clip or open a layer itself, because each save must be matched by its restore.
        if matches!(
            command,
            DrawCommand::Save
                | DrawCommand::Restore
                | DrawCommand::Opacity(_)
                | DrawCommand::OpacityLayer { .. }
                | DrawCommand::Transform(_)
                | DrawCommand::Clip { .. }
                | DrawCommand::EffectLayer { .. }
        ) {
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
    ) -> Vec<VariantId> {
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

    pub fn forget_surface(&mut self, surface: SurfaceId) {
        for definition in self.definitions.values_mut() {
            definition.backend.forget_surface(surface);
        }
    }

    pub fn render(&mut self, effect: EffectId, canvas: &Canvas, request: &EffectRequest<'_>) {
        if let Some(definition) = self.definitions.get_mut(&effect) {
            definition.backend.render(canvas, request);
        }
    }
}
