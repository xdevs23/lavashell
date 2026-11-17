// SPDX-License-Identifier: GPL-3.0-only
use super::StateError;
use crate::effect_registry::QualityChange;
use crate::{FrameOutcome, SurfaceContents, SurfaceTarget};
use lavashell_core::prelude::{DamageTracker, PixelScale, PixelSize, Presented, Rect, Viewport};
use lavashell_core::scene::tree::TreeId;

#[cfg(test)]
mod tests;

pub(super) struct AttachedSurface<C> {
    pub target: SurfaceTarget,
    pub control: C,
    pub outline: Option<skia_safe::Path>,
    pub damage: SurfaceDamage,
}

impl<C> AttachedSurface<C> {
    pub fn new(target: SurfaceTarget, control: C) -> Self {
        Self {
            target,
            control,
            outline: None,
            damage: SurfaceDamage::default(),
        }
    }

    pub fn set_outline(&mut self, outline: Option<skia_safe::Path>) {
        self.outline = outline;
        self.damage.invalidate();
    }

    pub fn damage(
        &mut self,
        shown: Presented<'_>,
        viewport: Viewport,
        physical: PixelSize,
        inputs: &PixelInputs,
    ) -> Result<Option<Rect>, StateError> {
        let target = &self.target;
        let contents = || target.contents(physical).map_err(StateError::ReadContents);
        self.damage.area(shown, viewport, inputs, contents)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DamageMode {
    Tracked,
    Whole,
}

pub(super) struct PixelInputs {
    pub lineage: TreeId,
    pub scene_generation: u64,
    pub quality: QualityChange,
}

#[derive(Default)]
pub(super) struct SurfaceDamage {
    tracker: DamageTracker,
    described: Option<(TreeId, u64)>,
    scale: Option<PixelScale>,
}

impl SurfaceDamage {
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

    pub fn draw_at(&mut self, scale: PixelScale) {
        if !self.drawn_at(scale) {
            self.invalidate();
            self.scale = Some(scale);
        }
    }

    pub fn drawn_at(&self, scale: PixelScale) -> bool {
        self.scale == Some(scale)
    }

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
