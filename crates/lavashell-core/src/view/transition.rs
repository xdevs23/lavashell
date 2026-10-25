// SPDX-License-Identifier: GPL-3.0-only
mod changing;
mod departure;
mod registry;

use super::{
    BTreeMap, BTreeSet, Brush, Color, Dp, ModifierElement, NodeId, Point, Rect, View, paint,
};
use crate::animation::{Animation, Composite, MotionSpec, Playing, composite_fields};
use crate::color::{ColorIn, ColorInterpolation};
use crate::effects::{Interpolated, Material, ShapedMaterial};
use crate::scene::tree::Presence;
use crate::shape::Corners;
use crate::units::NonNegativeDp;
pub(super) use changing::Changing;
pub(super) use departure::Departure;
pub(super) use registry::Transitions;
use std::rc::Rc;

/// How a transition moves a node from its old bounds to its new bounds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BoundsMotion {
    /// The node moves to its new bounds at once.
    Instant,
    /// The node and its content move and scale from the old bounds to the new bounds.
    #[default]
    Scale,
    /// The material, background, border and clips of the node move and resize from the old bounds
    /// to the new bounds. The content is laid out at the new size and moves with the node without
    /// being scaled.
    Reflow,
}

#[derive(Clone, Copy, Debug)]
pub struct Transition {
    pub spec: MotionSpec,
    pub bounds: BoundsMotion,
    /// How the background, the clip corners, the opacity and the materials of the node animate,
    /// with colors mixed by this interpolation. `None` makes them change at once, so only the
    /// bounds animate.
    pub appearance: Option<ColorInterpolation>,
    pub enter: Option<Presence>,
    pub exit: Option<Presence>,
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

    /// Animates the bounds of the node as [`BoundsMotion::Reflow`] describes.
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

    /// Animates the node out to `presence` when the node is removed. Until that animation ends, the
    /// node and its subtree paint as they were last composed and laid out, at the position that the
    /// node had among its siblings.
    #[must_use]
    pub fn exit(mut self, presence: Presence) -> Self {
        self.exit = Some(presence);
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

/// The background, clip corners, opacity, material and group material of a node in a transition.
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
    /// The fixed appearance that a transition animating only the bounds uses as its target. Because
    /// this target never changes, the appearance never animates.
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

/// The indices in the modifier chain of a node at which the animated background, clip, opacity and
/// materials paint.
pub(super) struct AnimatedSteps {
    pub background: Option<usize>,
    pub clip: Option<usize>,
    pub opacity: Option<usize>,
    pub present: MaterialSteps,
    /// The steps at which materials removed from the node paint until they have faded out.
    pub departing: MaterialSteps,
}

/// Where in the modifier chain of a node the material and the material group paint.
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
    /// The steps of `self`, with the previously recorded step in `last` for each material that
    /// `self` lacks.
    fn or(self, last: Self) -> Self {
        Self {
            material: self.material.or(last.material),
            group: self.group.or(last.group),
        }
    }

    /// The steps in `last` of the materials that `self` lacks and that still show in `appearance`.
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
    /// The animated visual of the node at `index` with `rect` as its target bounds, or `None` when
    /// the node has no transition. The view reads the current value of the transition while
    /// painting, unless the scene plays the motion in its slots. A modifier value that the
    /// transition cannot animate fails the paint and returns `None`.
    pub(super) fn visual(
        &mut self,
        index: usize,
        rect: Rect,
        playback: paint::Playback,
    ) -> Option<AnimatedVisual> {
        let node = &self.nodes[index];
        let transition = node.transition?;
        let elements = &node.modifier;
        let background = first(elements, |e| match e {
            ModifierElement::Background(Brush::Solid(color)) => Some(*color),
            _ => None,
        });
        let clip = first(elements, |e| match e {
            ModifierElement::Clip(corners) => Some(*corners),
            _ => None,
        });
        let opacity = first(elements, |e| match e {
            ModifierElement::Opacity(alpha) => Some(alpha.get()),
            _ => None,
        });
        let material = first(elements, |e| match e {
            ModifierElement::Material(material) => Some(material),
            _ => None,
        });
        let group = first(elements, |e| match e {
            ModifierElement::MaterialGroup { material, spacing } => Some((material, *spacing)),
            _ => None,
        });
        let interpolation = transition.appearance.unwrap_or_default();
        let appearance = VisualAppearance {
            background: (background.map_or(Color::TRANSPARENT, |(_, color)| color))
                .interpolated(interpolation),
            corners: clip.map(|(_, corners)| corners).unwrap_or_default(),
            opacity: opacity.map_or(1.0, |(_, alpha)| alpha),
            material: material
                .map(|(_, material)| Interpolated::new(material.clone(), interpolation)),
            group_material: group
                .map(|(_, (material, _))| Interpolated::new(material.clone(), interpolation)),
        };
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
            material: material.map(|(step, _)| step),
            group: group.map(|(step, (_, spacing))| GroupStep { step, spacing }),
        };
        let id = node.id;
        let refreshed = (self.transitions).refresh(&self.motion, id, transition, target, steps);
        let (animation, last) = match refreshed {
            Ok((animation, last)) => (animation.clone(), last),
            Err(error) => {
                self.fail_paint(error);
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
                background: background.map(|(step, _)| step),
                clip: clip.map(|(step, _)| step),
                opacity: opacity.map(|(step, _)| step),
                present: steps,
                departing,
            },
        })
    }
}

/// The step and the value of the first element in `elements` matched by `value`.
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
