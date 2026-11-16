// SPDX-License-Identifier: GPL-3.0-only
use super::group::Unwind;
use super::{Playback, Surfaces, material, opacity};
use crate::animation::{Animatable, Animation, Playing, Vector};
use crate::effects::{Material, ShapedMaterial};
use crate::geometry::{Point, Rect};
use crate::scene::tree::{Slot, TransformMotion};
use crate::scene::{Affine, Brush, DrawCommand, Transform};
use crate::scroll::ScrollState;
use crate::shape::{Corners, Radius, RoundedRect};
use crate::ui::{ModifierElement, NodeId};
use crate::units::{Dp, Fraction};
use crate::view::output::{GroupId, ScopeKind};
use crate::view::transition::{AnimatedVisual, GroupStep};
use crate::view::{HitClip, View};

pub(super) struct ModifierInput<'a> {
    pub transform: Transform,
    pub radius_scale: Transform,
    pub surfaces: Surfaces,
    pub visual: Option<&'a AnimatedVisual>,
    pub clips: &'a [HitClip],
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

    fn laid_out_at_position(&self, step: usize) -> Rect {
        let (pos, size) = (self.painted.pos, self.view.layout[self.index].layers[step]);
        Rect::new(pos.x, pos.y, size.width, size.height)
    }

    fn animated(
        &self,
        steps: fn(&AnimatedVisual) -> Option<usize>,
        step: usize,
    ) -> Option<&'a AnimatedVisual> {
        self.input
            .visual
            .filter(|visual| steps(visual) == Some(step))
    }

    fn draw(&mut self, command: DrawCommand) {
        self.view.painting.canvas.push(command);
    }

    fn departing(&mut self, departing: &Departing, step: usize) {
        if let Some(material) = departing.material(step) {
            let rect = self.input.surfaces.rect(self.laid_out_at_position(step));
            material::commands(rect, material, self.input.surfaces.stretch)
                .paint(self.view.painting.canvas.commands(), self.unwind);
        }
        let group = departing.group(step, self.painted.pos);
        self.painted.groups.extend(group);
    }

    fn element(&mut self, step: usize) {
        let laid_out = self.laid_out_at_position(step);
        let rect = self.input.surfaces.rect(laid_out);
        match &self.view.nodes[self.index].modifier[step] {
            ModifierElement::AnimatedOffset(animation) => {
                self.animated_offset(&animation.clone(), step);
            }
            ModifierElement::FollowScroll { scroll, travel } => {
                self.follow_scroll(&scroll.clone(), *travel, step);
            }
            ModifierElement::AnimatedOpacity(animation) => {
                self.animated_opacity(&animation.clone(), step);
            }
            ModifierElement::Material(material) => {
                let material = (self.animated(|v| v.steps.present.material, step))
                    .and_then(AnimatedVisual::material)
                    .unwrap_or(material);
                material::commands(rect, material, self.input.surfaces.stretch)
                    .paint(self.view.painting.canvas.commands(), self.unwind);
            }
            ModifierElement::MaterialGroup { material, spacing } => {
                let material = (self.animated(|v| v.steps.present.group.map(|g| g.step), step))
                    .and_then(AnimatedVisual::group_material)
                    .unwrap_or(material);
                self.painted.groups.push(material::Group {
                    material: material.clone(),
                    spacing: *spacing,
                    origin: self.painted.pos,
                });
            }
            ModifierElement::Effect { blur, filter } => {
                let effect = ScopeKind::effect(laid_out.union(rect), *blur, filter.clone());
                self.unwind.scope(&mut self.view.painting.canvas, effect);
            }
            ModifierElement::Clip(corners) => self.clip(*corners, rect, step),
            ModifierElement::ClipRect(bounds, corners) => {
                self.clip_rect(*bounds, *corners, laid_out);
            }
            ModifierElement::Opacity(alpha) => self.opacity(*alpha, step),
            ModifierElement::AnimatedBackground(color) => {
                let brush = color.color().into();
                self.draw(DrawCommand::Fill {
                    shape: rect.into(),
                    brush,
                });
            }
            ModifierElement::Background(brush) => {
                let brush = self.background(brush, step);
                self.draw(DrawCommand::Fill {
                    shape: rect.into(),
                    brush,
                });
            }
            ModifierElement::Border { stroke, corners } => {
                let shape = RoundedRect::new(rect, *corners).into();
                let stroke = *stroke;
                self.draw(DrawCommand::Stroke { shape, stroke });
            }
            ModifierElement::Backdrop { effect, corners } => {
                let shape = RoundedRect::new(rect, *corners).into();
                let effect = *effect;
                self.draw(DrawCommand::Backdrop { shape, effect });
            }
            ModifierElement::Shadow { shadow, corners } => {
                let shape = RoundedRect::new(rect, *corners).into();
                let command = DrawCommand::Shadow {
                    shape,
                    shadow: *shadow,
                };
                if shadow.inset {
                    self.unwind.push(command);
                } else {
                    self.draw(command);
                }
            }
            ModifierElement::Outline {
                stroke,
                corners,
                offset,
            } => self.unwind.push(DrawCommand::Stroke {
                shape: RoundedRect::new(rect, *corners).outset(*offset).into(),
                stroke: *stroke,
            }),
            ModifierElement::Padding(_)
            | ModifierElement::Width(_)
            | ModifierElement::Height(_)
            | ModifierElement::MinSize(_)
            | ModifierElement::MaxSize(_)
            | ModifierElement::MeasurementLimit(_)
            | ModifierElement::FillWidth
            | ModifierElement::FillHeight
            | ModifierElement::Island(_)
            | ModifierElement::Offset(_)
            | ModifierElement::Position(_)
            | ModifierElement::ShownWhen(_)
            | ModifierElement::Invalid { .. } => {}
        }
    }

    fn animated_opacity(&mut self, animation: &Animation<f64>, step: usize) {
        let opacity =
            (self.view).animated_slot(animation, Playback::InSlot, |alpha| *alpha, Playing::clone);
        self.open_group(step, Slot::Value(Affine::IDENTITY), opacity);
    }

    fn open_group(&mut self, step: usize, transform: Slot<Affine>, opacity: Slot<f64>) {
        let group = GroupId::Animated {
            node: self.id,
            step,
        };
        let outer = self.painted.transform;
        let open = self.view.open_group(group, transform, opacity, outer);
        self.unwind.close(open);
    }

    fn animated_offset(&mut self, animation: &Animation<Point>, step: usize) {
        let translation =
            |vector: &Vector<2>| Affine::translate(Point::default().at(*vector)).vector();
        let slot = self.view.animated_slot(
            animation,
            self.input.offsets,
            |offset| Affine::translate(*offset),
            |playing| {
                let end = Affine::translate(*playing.end());
                TransformMotion::Affine(playing.mapped(end, translation))
            },
        );
        let offset = animation.peek();
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

    fn background(&self, brush: &Brush, step: usize) -> Brush {
        match brush {
            Brush::Solid(color) => Brush::Solid(
                self.animated(|v| v.steps.background, step)
                    .map_or(*color, |v| v.visual.appearance.background.color),
            ),
            Brush::Gradient(_) => brush.clone().translated(self.painted.pos),
        }
    }

    fn clip(&mut self, corners: Corners, rect: Rect, step: usize) {
        let corners = match self.animated(|v| v.steps.clip, step) {
            Some(v) if v.transition.appearance.is_some() => {
                let scale = self.input.radius_scale.scale;
                (v.visual.appearance.corners).map(|r| Radius::new(r / scale.x, r / scale.y))
            }
            _ => corners.into(),
        };
        let (transform, clips) = (self.painted.transform, &mut self.painted.clips);
        (self.view).clip_scope(rect, corners, transform, clips, self.unwind);
    }

    fn clip_rect(&mut self, bounds: Rect, corners: Corners, laid_out: Rect) {
        let pos = self.painted.pos;
        let placed = Rect::new(
            pos.x + bounds.x,
            pos.y + bounds.y,
            bounds.width.max(Dp::ZERO),
            bounds.height.max(Dp::ZERO),
        );
        let rect = self.input.surfaces.placed(placed, laid_out);
        let (transform, clips) = (self.painted.transform, &mut self.painted.clips);
        (self.view).clip_scope(rect, corners.into(), transform, clips, self.unwind);
    }

    fn opacity(&mut self, alpha: Fraction, step: usize) {
        let alpha = (self.animated(|v| v.steps.opacity, step))
            .and_then(|v| Fraction::saturating(v.visual.appearance.opacity).ok())
            .unwrap_or(alpha);
        opacity::paint(&mut self.view.painting.canvas, alpha, self.unwind);
    }
}

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
