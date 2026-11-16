// SPDX-License-Identifier: GPL-3.0-only
mod above;
mod group;
mod kind;
pub(super) mod material;
mod modifiers;
pub(super) mod opacity;
mod presence;
pub(super) mod scale;
mod scroll;
mod siblings;
mod surfaces;
mod tooltip;

use super::interaction::StyledAppearance;
use super::output::{Canvas, Shown};
use super::parents::ROOT;
use super::{Hit, HitClip, TextMeasurer, View, accessibility};
use crate::geometry::{Point, Rect};
use crate::scene::{DrawCommand, Transform};
use crate::shape::{Corners, Shape};
use crate::style::{AnimatedAppearance, ContentColors};
use crate::ui::{Identity, ModifierElement, NodeId};
use crate::view::component::index_of;
use crate::view::transition::{AnimatedVisual, TextBlend};
pub(super) use above::Above;
use above::OpenAbove;
pub(super) use group::{OpenGroup, Playback, Unwind};
use kind::KindInput;
use modifiers::{ModifierInput, PaintedModifiers};
use presence::TransitionSpace;
use scroll::Scrolled;
pub(super) use siblings::SiblingOrders;
use std::collections::BTreeMap;
pub(super) use surfaces::Surfaces;

#[derive(Default)]
pub(super) struct PaintOutput {
    pub canvas: Canvas,
    pub above: Vec<Above>,
    pub shown: Shown,
    pub spare_unwinds: Vec<Unwind>,
    pub sibling_orders: SiblingOrders,
    pub interaction_visuals: BTreeMap<NodeId, StyledAppearance>,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum Source {
    #[default]
    Composition,
    Leaving,
}

#[derive(Clone, Copy)]
pub(super) struct Inherited<'a> {
    pub clips: &'a [HitClip],
    pub content_colors: &'a ContentColors,
    pub source: Source,
    pub composed_parent: usize,
    pub laid_out: Point,
}

impl Default for Inherited<'_> {
    fn default() -> Self {
        static NONE: ContentColors = ContentColors::NONE;
        Self {
            clips: &[],
            content_colors: &NONE,
            source: Source::default(),
            composed_parent: ROOT,
            laid_out: Point::default(),
        }
    }
}

impl Inherited<'_> {
    fn for_children(self, index: usize, shift: Point) -> Self {
        Self {
            laid_out: self.laid_out.offset(shift.x, shift.y),
            composed_parent: match self.source {
                Source::Composition => index,
                Source::Leaving => self.composed_parent,
            },
            ..self
        }
    }
}

impl View {
    pub(super) fn paint_inner(
        &mut self,
        index: usize,
        pos: Point,
        origin: Point,
        parent_transform: Transform,
        inherited: Inherited,
        text: &mut impl TextMeasurer,
    ) {
        let mut opened = self.open_layers(index, pos, origin, parent_transform, inherited);
        self.paint_content(index, &mut opened, inherited, text);
        self.close_layers(opened);
    }

    fn open_layers(
        &mut self,
        index: usize,
        pos: Point,
        origin: Point,
        parent_transform: Transform,
        inherited: Inherited,
    ) -> Opened {
        let mut unwind = self.painting.unwinding();
        let interaction = self.interaction_visual(index, inherited);
        let plan = self.plan_paint(
            index,
            pos,
            origin,
            parent_transform,
            inherited.laid_out,
            interaction.is_some(),
        );
        let OpenVisual {
            visual,
            presence,
            above,
            mut transform,
            surfaces,
            surface,
            decorated_surface,
            mut radius_scale,
        } = self.open_visual(index, &plan, parent_transform, inherited);
        let text_blend = visual.as_ref().and_then(AnimatedVisual::text_blend);
        scale::apply(
            &mut self.painting.canvas,
            self.nodes[index].visual_scale.get(),
            surface,
            &mut transform,
            &mut radius_scale,
            &mut unwind,
        );
        self.layout[index].rect = transform.rect(surface);
        self.painting
            .open_state(surface, interaction.as_ref(), &mut unwind);
        let state_material = Self::state_material(surface, interaction.as_ref(), surfaces.stretch);
        if let Some(material) = state_material {
            material.paint(self.painting.canvas.commands(), &mut unwind);
        }
        let input = ModifierInput {
            transform,
            radius_scale,
            surfaces,
            visual: visual.as_ref(),
            clips: inherited.clips,
            offsets: plan.offsets,
        };
        let painted = self.paint_modifiers(index, pos, &input, &mut unwind);
        Opened {
            unwind,
            interaction,
            presence,
            above,
            surfaces,
            decorated_surface,
            painted,
            text_blend,
        }
    }

    fn paint_content(
        &mut self,
        index: usize,
        opened: &mut Opened,
        inherited: Inherited,
        text: &mut impl TextMeasurer,
    ) {
        let id = self.nodes[index].id;
        let decorated_surface = opened.decorated_surface;
        let interaction = opened.interaction.as_ref();
        let (clips, transform) = (&mut opened.painted.clips, opened.painted.transform);
        let groups = std::mem::take(&mut opened.painted.groups);
        self.painting
            .paint_state_tint(decorated_surface, interaction);
        let s = self.layout[index].innermost_layer();
        let rect = Rect::new(
            opened.painted.pos.x,
            opened.painted.pos.y,
            s.width,
            s.height,
        );
        self.register_hit(index, clips);
        let scroll = self.scroll_content(index, Playback::in_slot_if(groups.is_empty()));
        if scroll.is_some() {
            let viewport = opened.surfaces.rect(rect);
            let unwind = &mut opened.unwind;
            self.clip_scope(viewport, Corners::default(), transform, clips, unwind);
        }
        let outer = (!groups.is_empty()).then(|| self.painting.open_items());
        let painted = self.painted_rect(index, rect, opened.text_blend, text);
        self.record_accessibility_geometry(index, painted, transform, clips);
        let pushed = (interaction.and_then(|state| state.content))
            .map(|content| inherited.content_colors.pushed(content));
        let at_rest = opened.painted.offset_at_rest;
        let inside = Inherited {
            clips,
            content_colors: pushed.as_ref().unwrap_or(inherited.content_colors),
            source: inherited.source,
            composed_parent: inherited.composed_parent,
            laid_out: inherited.laid_out.offset(at_rest.x, at_rest.y),
        };
        let input = KindInput {
            bounds: decorated_surface,
            rect: painted,
            transform,
            text_blend: opened.text_blend,
        };
        self.paint_kind(index, input, inside, text);
        if self.is_focused(id) && self.focus_shows_on(index) {
            self.paint_focus_ring(index, decorated_surface, interaction);
        }
        let shift = scroll.as_ref().map_or(Point::default(), Scrolled::resting);
        let content = scroll.map(|scroll| self.open_content(index, scroll, transform));
        let inner = content.as_ref().map_or(transform, |(_, inner)| *inner);
        let start = Point::new(rect.x, rect.y);
        self.paint_children(index, start, inner, inside.for_children(index, shift), text);
        if let Some((open, _)) = content {
            self.close_group(open);
        }
        if let Some(outer) = outer {
            let content = self.painting.close_items(outer);
            self.paint_groups(index, groups, transform, content);
        }
        if self.shows_focus_within(index) {
            self.paint_focus_within(index, decorated_surface);
        }
        self.painting
            .paint_state_outline(decorated_surface, interaction);
    }

    fn close_layers(&mut self, opened: Opened) {
        self.unwind(opened.unwind);
        if let Some(open) = opened.presence {
            self.close_group(open);
        }
        if let Some(open) = opened.above {
            self.close_above(open);
        }
    }

    fn open_visual(
        &mut self,
        index: usize,
        plan: &PaintPlan,
        parent_transform: Transform,
        inherited: Inherited,
    ) -> OpenVisual {
        let id = self.nodes[index].id;
        let visual = self.visual(
            index,
            plan.space.relative(plan.target_rect),
            plan.visual_playback,
            inherited.composed_parent,
        );
        let above = self.open_above(index, inherited.source, parent_transform);
        let (opened, presence) = match &visual {
            Some(animated) => {
                let (opened, group) = self.open_presence(
                    id,
                    animated,
                    plan.target_rect,
                    &plan.space,
                    parent_transform,
                );
                (Some(opened), Some(group))
            }
            None => (None, None),
        };
        let transform = opened.as_ref().map_or(parent_transform, |opened| {
            parent_transform.then(opened.local)
        });
        let surfaces = opened
            .as_ref()
            .map_or(Surfaces::STILL, |opened| opened.surfaces);
        OpenVisual {
            visual,
            presence,
            above,
            transform,
            surfaces,
            surface: surfaces.rect(plan.target_rect),
            decorated_surface: surfaces.rect(plan.unmoved),
            radius_scale: surfaces.stretch,
        }
    }

    fn record_accessibility_geometry(
        &mut self,
        index: usize,
        rect: Rect,
        transform: Transform,
        clips: &[HitClip],
    ) {
        let id = self.nodes[index].id;
        let geometry = accessibility::SemanticGeometry::new(
            self.layout[index].rect,
            transform.rect(rect),
            transform.scale,
            clips,
        );
        let _geometry_of_the_previous_frame = self.accessibility.geometry.insert(id, geometry);
    }

    fn plan_paint(
        &self,
        index: usize,
        pos: Point,
        origin: Point,
        parent_transform: Transform,
        laid_out: Point,
        interactive_look: bool,
    ) -> PaintPlan {
        let id = self.nodes[index].id;
        let node = &self.nodes[index];
        let offset = self.layout[index].offset;
        let placed = pos.offset(offset.x, offset.y);
        let animated = (node.modifier.iter()).fold(placed, |position, element| match element {
            ModifierElement::AnimatedOffset(a) => {
                let p = a.peek();
                position.offset(p.x, p.y)
            }
            _ => position,
        });
        let size = self.layout[index].size;
        let at = |position: Point| Rect::new(position.x, position.y, size.width, size.height);
        let target_rect = at(animated);
        let unmoved = at(placed);
        let island =
            (node.modifier.iter()).any(|element| matches!(element, ModifierElement::Island(_)));
        let decorated = interactive_look
            || node.interaction.focus.is_some() && self.is_focused(id)
            || node.focus_within.is_some();
        let offsets = Playback::in_slot_if(node.transition.is_none() && !decorated && !island);
        let visual_playback = Playback::in_slot_if(!island);
        let space = match node.identity {
            Identity::InView => {
                let resting = self.routing.moving.at_rest(parent_transform);
                TransitionSpace::in_view(resting, laid_out, self.viewport_origin)
            }
            Identity::InParent => TransitionSpace::in_parent(origin),
        };
        PaintPlan {
            target_rect,
            unmoved,
            offsets,
            visual_playback,
            space,
        }
    }
}

struct Opened {
    unwind: Unwind,
    interaction: Option<AnimatedAppearance>,
    presence: Option<OpenGroup>,
    above: Option<OpenAbove>,
    surfaces: Surfaces,
    decorated_surface: Rect,
    painted: PaintedModifiers,
    text_blend: Option<TextBlend>,
}

struct OpenVisual {
    visual: Option<AnimatedVisual>,
    presence: Option<OpenGroup>,
    above: Option<OpenAbove>,
    transform: Transform,
    surfaces: Surfaces,
    surface: Rect,
    decorated_surface: Rect,
    radius_scale: Transform,
}

struct PaintPlan {
    target_rect: Rect,
    unmoved: Rect,
    offsets: Playback,
    visual_playback: Playback,
    space: TransitionSpace,
}

impl View {
    fn register_hit(&mut self, index: usize, clips: &[HitClip]) {
        let node = &self.nodes[index];
        if node.modal {
            self.routing.hits.clear();
        }
        if node.enabled && (node.handles_input() || node.modal) {
            let rect = self.layout[index].rect;
            let mut hit = Hit {
                group: self.routing.moving.current,
                ..Hit::new(node.id, node.focusable && !node.modal, rect, clips.to_vec())
            };
            hit.use_handlers_of(node);
            self.routing.hits.push(hit);
        }
    }

    pub(super) fn is_focused(&self, id: NodeId) -> bool {
        self.window_focused && self.routing.focus == Some(id)
    }

    fn shows_focus_within(&self, index: usize) -> bool {
        let focused = (self.routing.focus.filter(|_| self.window_focused))
            .and_then(|id| index_of(&self.nodes, id));
        focused.is_some_and(|focused| {
            self.focus_shows_on(focused) && self.parents.ancestry(focused).any(|node| node == index)
        })
    }

    fn paint_focus_ring(&mut self, index: usize, rect: Rect, state: Option<&AnimatedAppearance>) {
        let node = &self.nodes[index];
        let outlined = state.is_some_and(|appearance| appearance.outline.is_some());
        if !outlined && let Some(focus) = &node.interaction.focus {
            self.painting.canvas.push(DrawCommand::Stroke {
                shape: Shape::rounded(rect, focus.radius),
                stroke: focus.stroke(),
            });
        }
    }

    fn paint_focus_within(&mut self, index: usize, rect: Rect) {
        if let Some(border) = &self.nodes[index].focus_within {
            self.painting.canvas.push(DrawCommand::Stroke {
                shape: Shape::rounded(rect, border.radius),
                stroke: border.stroke(),
            });
        }
    }
}
