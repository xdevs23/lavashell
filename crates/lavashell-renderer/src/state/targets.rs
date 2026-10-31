// SPDX-License-Identifier: GPL-3.0-only
use super::StateError;
use crate::effect_registry::QualityChange;
use crate::{FrameOutcome, SurfaceContents, SurfaceTarget};
use lavashell_core::prelude::{DamageTracker, Presented, Rect, Viewport};
use lavashell_core::scene::tree::TreeId;
use std::time::Duration;

#[cfg(test)]
mod tests;

/// A surface attached to the renderer state: its target, its control and the damage records of the pixels in its
/// target.
pub(super) struct AttachedSurface<C> {
    pub target: SurfaceTarget,
    pub control: C,
    pub gpu_budget: Option<Duration>,
    /// An outline in physical pixels that clips the drawing of every frame.
    pub outline: Option<skia_safe::Path>,
    pub damage: SurfaceDamage,
}

impl<C> AttachedSurface<C> {
    pub fn new(target: SurfaceTarget, control: C, gpu_budget: Option<Duration>) -> Self {
        Self {
            target,
            control,
            gpu_budget,
            outline: None,
            damage: SurfaceDamage::default(),
        }
    }

    pub fn set_outline(&mut self, outline: Option<skia_safe::Path>) {
        self.outline = outline;
        self.damage.invalidate();
    }

    /// The area of `shown` to paint at `physical` pixels: the whole viewport with `full_redraw`, otherwise the area
    /// that differs from the pixels in the target.
    pub fn damage(
        &mut self,
        shown: Presented<'_>,
        viewport: Viewport,
        physical: (u32, u32),
        inputs: &PixelInputs,
        full_redraw: bool,
    ) -> Result<Option<Rect>, StateError> {
        let target = &self.target;
        let contents = || target.contents(physical).map_err(StateError::ReadContents);
        let area = self.damage.area(shown, viewport, inputs, contents)?;
        Ok(if full_redraw {
            Some(Rect::from_size(viewport.size))
        } else {
            area
        })
    }
}

/// What the pixels of a frame depend on, apart from the nodes of its scene and the pixels left in the target.
pub(super) struct PixelInputs {
    pub lineage: TreeId,
    pub scene_generation: u64,
    pub quality: QualityChange,
}

/// The damage records of the pixels in a target, with the lineage and the scene image of the frame described by the
/// records.
#[derive(Default)]
pub(super) struct SurfaceDamage {
    tracker: DamageTracker,
    described: Option<(TreeId, u64)>,
}

impl SurfaceDamage {
    /// The area of `shown` that differs from the pixels in the target. The whole viewport differs when the lineage or
    /// the scene image differ from those of the last frame, when the quality of an effect changed, or when `contents`
    /// reports that the target lost its pixels. `contents` runs only when the other checks find no change.
    pub fn area<E>(
        &mut self,
        shown: Presented<'_>,
        viewport: Viewport,
        inputs: &PixelInputs,
        contents: impl FnOnce() -> Result<SurfaceContents, E>,
    ) -> Result<Option<Rect>, E> {
        let described = (inputs.lineage, inputs.scene_generation);
        if self.described != Some(described)
            || inputs.quality == QualityChange::Changed
            || contents()? == SurfaceContents::Outdated
        {
            self.invalidate();
            self.described = Some(described);
        }
        Ok(self.tracker.damage(shown, viewport))
    }

    /// Forgets the records after a frame that was not presented, because the records describe that frame.
    pub fn forget_unless_shown(&mut self, outcome: FrameOutcome) {
        if outcome != FrameOutcome::Shown {
            self.invalidate();
        }
    }

    pub fn invalidate(&mut self) {
        self.tracker.invalidate();
        self.described = None;
    }
}
