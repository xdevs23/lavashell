// SPDX-License-Identifier: GPL-3.0-only
mod changing;
mod departure;
mod registry;

use super::{View, paint};
use crate::animation::{Composite, MotionSpec, Playing, composite_fields};
use crate::color::{ColorIn, ColorInterpolation};
use crate::effects::{Interpolated, Material, ShapedMaterial};
use crate::geometry::{Color, Rect};
use crate::scene::Brush;
use crate::scene::tree::Presence;
use crate::shape::Corners;
use crate::ui::ModifierElement;
use crate::units::{Dp, NonNegativeDp};
pub(super) use changing::Changing;
pub(super) use departure::Departure;
pub(super) use registry::Transitions;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BoundsMotion {
    Instant,
    #[default]
    Scale,
    Reflow,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Exit {
    pub presence: Presence,
    pub spec: MotionSpec,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transition {
    pub spec: MotionSpec,
    pub bounds: BoundsMotion,
    pub appearance: Option<ColorInterpolation>,
    pub enter: Option<Presence>,
    pub exit: Option<Exit>,
}

impl Transition {
    pub fn new(spec: impl Into<MotionSpec>) -> Self {
        Self {
            spec: spec.into(),
            bounds: BoundsMotion::default(),
            appearance: Some(ColorInterpolation::default()),
            enter: None,
            exit: None,
        }
    }

    #[must_use]
    pub fn reflow(mut self) -> Self {
        self.bounds = BoundsMotion::Reflow;
        self
    }

    #[must_use]
    pub fn enter(mut self, presence: Presence) -> Self {
        self.enter = Some(presence);
        self
    }

    #[must_use]
    pub fn exit(self, presence: Presence) -> Self {
        let spec = self.spec;
        self.exit_with(presence, spec)
    }

    #[must_use]
    pub fn exit_with(mut self, presence: Presence, spec: impl Into<MotionSpec>) -> Self {
        let spec = spec.into();
        self.exit = Some(Exit { presence, spec });
        self
    }

    pub(super) fn exits(&self) -> bool {
        self.exit.is_some()
    }

    #[must_use]
    pub fn bounds_only(mut self) -> Self {
        self.appearance = None;
        self
    }

    #[must_use]
    pub fn color_interpolation(mut self, interpolation: ColorInterpolation) -> Self {
        self.appearance = Some(interpolation);
        self
    }
}

impl From<MotionSpec> for Transition {
    fn from(spec: MotionSpec) -> Self {
        Self::new(spec)
    }
}

impl From<crate::animation::Spring> for Transition {
    fn from(spec: crate::animation::Spring) -> Self {
        Self::new(spec)
    }
}

impl From<crate::animation::Tween> for Transition {
    fn from(spec: crate::animation::Tween) -> Self {
        Self::new(spec)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Visual {
    pub rect: Rect,
    pub presence: Presence,
    pub appearance: VisualAppearance,
}

composite_fields! {
    pub(super) struct VisualParts {
        pub(super) rect: Rect,
        pub(super) presence: Presence,
        pub(super) appearance: VisualAppearance,
    }
    vectors VisualVectors
}

impl Composite for Visual {
    type Parts = VisualParts;

    fn parts(&self) -> VisualParts {
        VisualParts {
            rect: self.rect,
            presence: self.presence,
            appearance: self.appearance.clone(),
        }
    }

    fn with_parts(&self, parts: VisualParts) -> Self {
        let rect = parts.rect;
        Self {
            rect: Rect::new(
                rect.x,
                rect.y,
                rect.width.max(Dp::ZERO),
                rect.height.max(Dp::ZERO),
            ),
            presence: parts.presence,
            appearance: parts.appearance,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct VisualAppearance {
    pub background: ColorIn,
    pub corners: Corners,
    pub opacity: f64,
    pub material: Option<Interpolated<ShapedMaterial>>,
    pub group_material: Option<Interpolated<Material>>,
}

composite_fields! {
    pub(super) struct VisualAppearanceParts {
        background: ColorIn,
        corners: Corners,
        opacity: f64,
        material: Option<Interpolated<ShapedMaterial>>,
        group_material: Option<Interpolated<Material>>,
    }
}

impl VisualAppearance {
    fn still() -> Self {
        Self {
            background: Color::TRANSPARENT.interpolated(ColorInterpolation::default()),
            corners: Corners::default(),
            opacity: 1.0,
            material: None,
            group_material: None,
        }
    }
}

impl Composite for VisualAppearance {
    type Parts = VisualAppearanceParts;

    fn parts(&self) -> VisualAppearanceParts {
        VisualAppearanceParts {
            background: self.background,
            corners: self.corners,
            opacity: self.opacity,
            material: self.material.clone(),
            group_material: self.group_material.clone(),
        }
    }

    fn with_parts(&self, parts: VisualAppearanceParts) -> Self {
        Self {
            background: parts.background,
            corners: parts.corners,
            opacity: parts.opacity.clamp(0.0, 1.0),
            material: parts.material,
            group_material: parts.group_material,
        }
    }
}

pub(super) struct AnimatedSteps {
    pub background: Option<usize>,
    pub clip: Option<usize>,
    pub opacity: Option<usize>,
    pub present: MaterialSteps,
    pub departing: MaterialSteps,
}

#[derive(Clone, Copy, Default)]
pub(super) struct MaterialSteps {
    pub material: Option<usize>,
    pub group: Option<GroupStep>,
}

#[derive(Clone, Copy)]
pub(super) struct GroupStep {
    pub step: usize,
    pub spacing: NonNegativeDp,
}

impl MaterialSteps {
    fn or(self, last: Self) -> Self {
        Self {
            material: self.material.or(last.material),
            group: self.group.or(last.group),
        }
    }

    fn departing(self, last: Self, appearance: &VisualAppearance) -> Self {
        Self {
            material: last
                .material
                .filter(|_| self.material.is_none() && appearance.material.is_some()),
            group: last
                .group
                .filter(|_| self.group.is_none() && appearance.group_material.is_some()),
        }
    }
}

pub(super) struct AnimatedVisual {
    pub visual: Visual,
    pub transition: Transition,
    pub steps: AnimatedSteps,
    pub motion: Option<(Rc<Playing<Visual>>, Changing)>,
}

impl AnimatedVisual {
    pub fn material(&self) -> Option<&ShapedMaterial> {
        self.visual
            .appearance
            .material
            .as_ref()
            .map(Interpolated::value)
    }

    pub fn group_material(&self) -> Option<&Material> {
        self.visual
            .appearance
            .group_material
            .as_ref()
            .map(Interpolated::value)
    }
}

impl View {
    pub(super) fn visual(
        &mut self,
        index: usize,
        rect: Rect,
        playback: paint::Playback,
        composed_parent: usize,
    ) -> Option<AnimatedVisual> {
        let node = &self.nodes[index];
        let transition = node.transition?;
        let found = FoundElements::of(&node.modifier);
        let appearance = found.appearance(transition.appearance.unwrap_or_default());
        let target = Visual {
            rect,
            presence: Presence::default(),
            appearance: if transition.appearance.is_some() {
                appearance.clone()
            } else {
                VisualAppearance::still()
            },
        };
        let steps = MaterialSteps {
            material: found.material.map(|(step, _)| step),
            group: (found.group).map(|(step, (_, spacing))| GroupStep { step, spacing }),
        };
        let id = node.id;
        let refreshed = (self.transitions).refresh(&self.motion, id, &transition, target, steps);
        let (animation, last) = match refreshed {
            Ok((animation, last)) => (animation.clone(), last),
            Err(error) => {
                self.failures.fail_paint(composed_parent, error);
                return None;
            }
        };
        let motion = self.slot_motion(&animation, playback).and_then(|playing| {
            let changing = Changing::of(playing.program())?;
            (!changing.reshapes).then_some((playing, changing))
        });
        let mut visual = match motion {
            Some(_) => animation.peek(),
            None => animation.get(),
        };
        if transition.appearance.is_none() {
            visual.appearance = appearance;
        }
        let departing = steps.departing(last, &visual.appearance);
        Some(AnimatedVisual {
            visual,
            transition,
            motion,
            steps: AnimatedSteps {
                background: found.background.map(|(step, _)| step),
                clip: found.clip.map(|(step, _)| step),
                opacity: found.opacity.map(|(step, _)| step),
                present: steps,
                departing,
            },
        })
    }
}

struct FoundElements<'a> {
    background: Option<(usize, Color)>,
    clip: Option<(usize, Corners)>,
    opacity: Option<(usize, f64)>,
    material: Option<(usize, &'a ShapedMaterial)>,
    group: Option<(usize, (&'a Material, NonNegativeDp))>,
}

impl<'a> FoundElements<'a> {
    fn of(elements: &'a [ModifierElement]) -> Self {
        Self {
            background: first(elements, |e| match e {
                ModifierElement::Background(Brush::Solid(color)) => Some(*color),
                _ => None,
            }),
            clip: first(elements, |e| match e {
                ModifierElement::Clip(corners) => Some(*corners),
                _ => None,
            }),
            opacity: first(elements, |e| match e {
                ModifierElement::Opacity(alpha) => Some(alpha.get()),
                _ => None,
            }),
            material: first(elements, |e| match e {
                ModifierElement::Material(material) => Some(material),
                _ => None,
            }),
            group: first(elements, |e| match e {
                ModifierElement::MaterialGroup { material, spacing } => Some((material, *spacing)),
                _ => None,
            }),
        }
    }

    fn appearance(&self, interpolation: ColorInterpolation) -> VisualAppearance {
        VisualAppearance {
            background: (self
                .background
                .map_or(Color::TRANSPARENT, |(_, color)| color))
            .interpolated(interpolation),
            corners: self.clip.map(|(_, corners)| corners).unwrap_or_default(),
            opacity: self.opacity.map_or(1.0, |(_, alpha)| alpha),
            material: (self.material)
                .map(|(_, material)| Interpolated::new(material.clone(), interpolation)),
            group_material: (self.group)
                .map(|(_, (material, _))| Interpolated::new(material.clone(), interpolation)),
        }
    }
}

fn first<'a, T>(
    elements: &'a [ModifierElement],
    value: impl Fn(&'a ModifierElement) -> Option<T>,
) -> Option<(usize, T)> {
    (elements.iter().enumerate()).find_map(|(step, element)| value(element).map(|v| (step, v)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::DpExt;

    #[test]
    fn an_animated_size_below_zero_collapses_to_zero_instead_of_mirroring() {
        let visual = Visual {
            rect: Rect::new(1.dp(), 2.dp(), 30.dp(), 40.dp()),
            presence: Presence::default(),
            appearance: VisualAppearance::still(),
        };
        let mut parts = visual.parts();
        parts.rect = Rect::new(1.dp(), 2.dp(), -5.dp(), -0.5.dp());
        let moved = visual.with_parts(parts);
        assert_eq!(moved.rect, Rect::new(1.dp(), 2.dp(), Dp::ZERO, Dp::ZERO));
    }
}
