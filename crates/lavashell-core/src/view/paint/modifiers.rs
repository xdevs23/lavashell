// SPDX-License-Identifier: GPL-3.0-only
use super::group::Unwind;
use super::{
    Affine, Animatable, Brush, Dp, DrawCommand, HitClip, ModifierElement, NodeId, Playback, Point,
    Radius, Rect, RoundedRect, Surfaces, Transform, View, material, opacity,
};
use crate::animation::{Animation, Playing, Vector};
use crate::effects::{Material, ShapedMaterial};
use crate::scene::tree::{Slot, TransformMotion};
use crate::scroll::ScrollState;
use crate::units::Fraction;
use crate::view::output::{GroupId, ScopeKind};
use crate::view::transition::{AnimatedVisual, GroupStep};

pub(super) struct ModifierInput<'a> {
    pub transform: Transform,
    /// The transform by whose scale the view divides the radii of an animated clip, so that the radii do not grow or
    /// shrink with the animated bounds of the node.
    pub radius_scale: Transform,
    pub surfaces: Surfaces,
    pub visual: Option<&'a AnimatedVisual>,
    pub clips: &'a [HitClip],
    /// `InSlot` unless the node paints something outside the groups of its animated offsets that has to move with
    /// those offsets, such as a focus ring.
    pub offsets: Playback,
}

pub(super) struct PaintedModifiers {
    pub pos: Point,
    pub transform: Transform,
    pub offset_at_rest: Point,
    pub clips: Vec<HitClip>,
    pub groups: Vec<material::Group>,
}

impl View {
    /// Paints the modifiers of the node at `index`, which its parent places at `slot`.
    pub(super) fn paint_modifiers(
        &mut self,
        index: usize,
        slot: Point,
        input: &ModifierInput,
        unwind: &mut Unwind,
    ) -> PaintedModifiers {
        let id = self.nodes[index].id;
        let end = self.nodes[index].modifier.len();
        let departing = Departing::of(input.visual, end);
        let mut painter = ModifierPainter {
            view: self,
            index,
            id,
            slot,
            input,
            unwind,
            painted: PaintedModifiers {
                pos: slot,
                transform: input.transform,
                offset_at_rest: Point::default(),
                clips: input.clips.to_vec(),
                groups: Vec::new(),
            },
        };
        for step in 0..end {
            painter.move_to_layer(step);
            painter.departing(&departing, step);
            painter.element(step);
        }
        painter.move_to_layer(end);
        painter.departing(&departing, end);
        painter.painted
    }
}

struct ModifierPainter<'a> {
    view: &'a mut View,
    index: usize,
    id: NodeId,
    slot: Point,
    input: &'a ModifierInput<'a>,
    unwind: &'a mut Unwind,
    painted: PaintedModifiers,
}

impl<'a> ModifierPainter<'a> {
    fn move_to_layer(&mut self, step: usize) {
        let start = self.view.layout[self.index].starts[step];
        self.painted.pos = self.slot.offset(start.x, start.y);
    }

    /// The bounds of the layer at `step` at the current position, as laid out and as painted.
    fn bounds_at_position(&self, step: usize) -> (Rect, Rect) {
        let (pos, size) = (self.painted.pos, self.view.layout[self.index].layers[step]);
        let laid_out = Rect::new(pos.x, pos.y, size.width, size.height);
        (laid_out, self.input.surfaces.rect(laid_out))
    }

    /// The animated visual of the node when the step returned by `steps` for that visual is `step`.
    fn animated(
        &self,
        steps: fn(&AnimatedVisual) -> Option<usize>,
        step: usize,
    ) -> Option<&'a AnimatedVisual> {
        self.input
            .visual
            .filter(|visual| steps(visual) == Some(step))
    }

    fn departing(&mut self, departing: &Departing, step: usize) {
        if let Some(material) = departing.material(step) {
            let (_, rect) = self.bounds_at_position(step);
            material::commands(rect, material, self.input.surfaces.stretch)
                .paint(self.view.canvas.commands(), self.unwind);
        }
        let group = departing.group(step, self.painted.pos);
        self.painted.groups.extend(group);
    }

    fn element(&mut self, step: usize) {
        match &self.view.nodes[self.index].modifier[step] {
            ModifierElement::AnimatedOffset(animation) => {
                let animation = animation.clone();
                self.animated_offset(&animation, step);
            }
            ModifierElement::FollowScroll { scroll, travel } => {
                let (scroll, travel) = (scroll.clone(), *travel);
                self.follow_scroll(&scroll, travel, step);
            }
            ModifierElement::AnimatedOpacity(animation) => {
                let animation = animation.clone();
                let (opacity, _) = (self.view).animated_slot(
                    &animation,
                    Playback::InSlot,
                    |alpha| *alpha,
                    Playing::clone,
                );
                self.open_group(step, Slot::Value(Affine::IDENTITY), opacity);
            }
            ModifierElement::Material(_) | ModifierElement::MaterialGroup { .. } => {
                self.material(step);
            }
            ModifierElement::Effect { .. }
            | ModifierElement::Clip(_)
            | ModifierElement::ClipRect(..)
            | ModifierElement::Opacity(_) => self.layer(step),
            _ => self.decoration(step),
        }
    }

    /// Opens the group of the modifier at `step`. The group closes when the view unwinds the node.
    fn open_group(&mut self, step: usize, transform: Slot<Affine>, opacity: Slot<f64>) {
        let group = GroupId::Animated {
            node: self.id,
            step,
        };
        let outer = self.painted.transform;
        let open = self.view.open_group(group, transform, opacity, outer);
        self.unwind.close(open);
    }

    /// Moves everything painted after `step` by the animated offset, inside a group that plays the motion of that
    /// offset.
    fn animated_offset(&mut self, animation: &Animation<Point>, step: usize) {
        let translation =
            |vector: &Vector<2>| Affine::translate(Point::default().at(*vector)).vector();
        let (slot, offset) = self.view.animated_slot(
            animation,
            self.input.offsets,
            |offset| Affine::translate(*offset),
            |playing| {
                let end = Affine::translate(*playing.end());
                TransformMotion::Affine(playing.mapped(end, translation))
            },
        );
        let end = slot.end().translation;
        self.painted.offset_at_rest = self.painted.offset_at_rest.offset(end.x, end.y);
        self.open_group(step, slot, Slot::Value(1.0));
        self.painted.transform = (self.painted.transform).then(Transform::translation(offset));
    }

    fn follow_scroll(&mut self, scroll: &ScrollState, travel: Dp, step: usize) {
        let group = GroupId::Animated {
            node: self.id,
            step,
        };
        let follow = (self.view).follow_scroll(group, scroll, travel, self.input.offsets);
        let (end, shift) = (follow.resting(), follow.shift);
        self.painted.offset_at_rest = self.painted.offset_at_rest.offset(end.x, end.y);
        let open = (self.view).open_scrolled(group, follow, self.painted.transform);
        self.unwind.close(open);
        self.painted.transform = (self.painted.transform).then(Transform::translation(shift));
    }

    /// Paints a fill, a stroke, a shadow or a backdrop.
    fn decoration(&mut self, step: usize) {
        let (_, rect) = self.bounds_at_position(step);
        let command = match &self.view.nodes[self.index].modifier[step] {
            ModifierElement::AnimatedBackground(color) => DrawCommand::Fill {
                shape: rect.into(),
                brush: color.color().into(),
            },
            ModifierElement::Background(brush) => {
                let brush = match brush {
                    Brush::Solid(color) => Brush::Solid(
                        self.animated(|v| v.steps.background, step)
                            .map_or(*color, |v| v.visual.appearance.background.color),
                    ),
                    Brush::Gradient(_) => brush.clone().translated(self.painted.pos),
                };
                DrawCommand::Fill {
                    shape: rect.into(),
                    brush,
                }
            }
            ModifierElement::Border { stroke, corners } => DrawCommand::Stroke {
                shape: RoundedRect::new(rect, *corners).into(),
                stroke: *stroke,
            },
            ModifierElement::Backdrop { effect, corners } => DrawCommand::Backdrop {
                shape: RoundedRect::new(rect, *corners).into(),
                effect: *effect,
            },
            ModifierElement::Shadow { shadow, corners } if shadow.inset => {
                self.unwind.push(DrawCommand::Shadow {
                    shape: RoundedRect::new(rect, *corners).into(),
                    shadow: *shadow,
                });
                return;
            }
            ModifierElement::Shadow { shadow, corners } => DrawCommand::Shadow {
                shape: RoundedRect::new(rect, *corners).into(),
                shadow: *shadow,
            },
            ModifierElement::Outline {
                stroke,
                corners,
                offset,
            } => {
                self.unwind.push(DrawCommand::Stroke {
                    shape: RoundedRect::new(rect, *corners).outset(*offset).into(),
                    stroke: *stroke,
                });
                return;
            }
            _ => return,
        };
        self.view.canvas.push(command);
    }

    /// Paints a material, or records a material group, which `paint_groups` paints around the islands among the
    /// children.
    fn material(&mut self, step: usize) {
        let (_, rect) = self.bounds_at_position(step);
        match &self.view.nodes[self.index].modifier[step] {
            ModifierElement::Material(material) => {
                let material = self
                    .animated(|v| v.steps.present.material, step)
                    .and_then(AnimatedVisual::material)
                    .unwrap_or(material);
                material::commands(rect, material, self.input.surfaces.stretch)
                    .paint(self.view.canvas.commands(), self.unwind);
            }
            ModifierElement::MaterialGroup { material, spacing } => {
                let material = self
                    .animated(|v| v.steps.present.group.map(|group| group.step), step)
                    .and_then(AnimatedVisual::group_material)
                    .unwrap_or(material);
                self.painted.groups.push(material::Group {
                    material: material.clone(),
                    spacing: *spacing,
                    origin: self.painted.pos,
                });
            }
            _ => {}
        }
    }

    /// Opens an effect, a clip or an opacity that applies until the unwind.
    fn layer(&mut self, step: usize) {
        let (laid_out, rect) = self.bounds_at_position(step);
        let (pos, transform) = (self.painted.pos, self.painted.transform);
        match &self.view.nodes[self.index].modifier[step] {
            ModifierElement::Effect { blur, filter } => {
                let effect = ScopeKind::effect(laid_out.union(rect), *blur, filter.clone());
                self.unwind.scope(&mut self.view.canvas, effect);
            }
            ModifierElement::Clip(corners) => {
                let corners = match self.animated(|v| v.steps.clip, step) {
                    // Dividing out the scale of the animated bounds paints the animated
                    // radius at every size of the bounds.
                    Some(v) if v.transition.appearance.is_some() => {
                        let scale = self.input.radius_scale.scale;
                        (v.visual.appearance.corners).map(|r| Radius::new(r / scale.x, r / scale.y))
                    }
                    _ => (*corners).into(),
                };
                let clips = &mut self.painted.clips;
                (self.view).clip_scope(rect, corners, transform, clips, self.unwind);
            }
            ModifierElement::ClipRect(bounds, corners) => {
                let placed = Rect::new(
                    pos.x + bounds.x,
                    pos.y + bounds.y,
                    bounds.width.max(Dp::ZERO),
                    bounds.height.max(Dp::ZERO),
                );
                let rect = self.input.surfaces.placed(placed, laid_out);
                let corners = (*corners).into();
                let clips = &mut self.painted.clips;
                (self.view).clip_scope(rect, corners, transform, clips, self.unwind);
            }
            ModifierElement::Opacity(alpha) => {
                // An animated opacity that is NaN paints the opacity set in the modifier, which is the target of the
                // animation.
                let alpha = (self.animated(|v| v.steps.opacity, step))
                    .and_then(|v| Fraction::saturating(v.visual.appearance.opacity).ok())
                    .unwrap_or(*alpha);
                opacity::paint(&mut self.view.canvas, alpha, self.unwind);
            }
            _ => {}
        }
    }
}

/// The materials removed from the modifier of a node, each with the step at which it paints until it has faded out.
/// A step past the end of the modifier paints after the last element of the modifier.
struct Departing<'a> {
    material: Option<(usize, &'a ShapedMaterial)>,
    group: Option<(GroupStep, &'a Material)>,
}

impl<'a> Departing<'a> {
    fn of(visual: Option<&'a AnimatedVisual>, end: usize) -> Self {
        let steps = visual
            .map(|visual| visual.steps.departing)
            .unwrap_or_default();
        Self {
            material: (steps
                .material
                .zip(visual.and_then(AnimatedVisual::material)))
            .map(|(step, material)| (step.min(end), material)),
            group: (steps
                .group
                .zip(visual.and_then(AnimatedVisual::group_material)))
            .map(|(group, material)| {
                (
                    GroupStep {
                        step: group.step.min(end),
                        ..group
                    },
                    material,
                )
            }),
        }
    }

    fn material(&self, step: usize) -> Option<&'a ShapedMaterial> {
        (self.material).and_then(|(at, material)| (at == step).then_some(material))
    }

    fn group(&self, step: usize, origin: Point) -> Option<material::Group> {
        let (group, material) = self.group.filter(|(group, _)| group.step == step)?;
        Some(material::Group {
            material: material.clone(),
            spacing: group.spacing,
            origin,
        })
    }
}
