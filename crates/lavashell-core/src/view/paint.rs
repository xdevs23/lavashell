// SPDX-License-Identifier: GPL-3.0-only
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

use super::{
    Affine, BTreeMap, BTreeSet, Brush, Dp, DrawCommand, Drawing, FieldGeometry, Hit, HitClip,
    ModifierElement, NodeId, NodeKind, Point, Rect, Scale, Size, TextMeasurer, TextStyle,
    Transform, View, accessibility, field, index_of,
};
use crate::animation::Animatable;
use crate::shape::{Corners, Radius, RoundedRect, Shape};
use crate::style::{AnimatedAppearance, ContentColors};
use crate::ui::Identity;
use crate::view::transition::AnimatedVisual;
pub(super) use group::{OpenGroup, Playback, Unwind};
use modifiers::ModifierInput;
use presence::TransitionSpace;
use scroll::Scrolled;
pub(super) use siblings::SiblingOrders;
pub(super) use surfaces::Surfaces;

/// The tree from which the view paints a node.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum Source {
    #[default]
    Composition,
    /// The tree of a removed node, painted until the exit animation of that node ends.
    Leaving,
}

/// The values passed from a parent to each child while the view paints: the clips, the content colors, the source
/// tree and the laid-out origin.
#[derive(Clone, Copy)]
pub(super) struct Inherited<'a> {
    pub clips: &'a [HitClip],
    pub content_colors: &'a ContentColors,
    pub source: Source,
    /// The origin of the paint coordinates of the node in the view as laid out: without presence transitions and scale
    /// animations, and with every animated slot at its end value.
    pub laid_out: Point,
}

impl Default for Inherited<'_> {
    fn default() -> Self {
        static NONE: ContentColors = ContentColors::NONE;
        Self {
            clips: &[],
            content_colors: &NONE,
            source: Source::default(),
            laid_out: Point::default(),
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
        let mut unwind = self.unwinding();
        let id = self.nodes[index].id;
        let interaction = self.interaction_visual(index, inherited.source);
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
            mut transform,
            surfaces,
            surface,
            decorated_surface,
            mut radius_scale,
        } = self.open_visual(index, &plan, parent_transform);
        scale::apply(
            &mut self.canvas,
            self.nodes[index].visual_scale.get(),
            surface,
            &mut transform,
            &mut radius_scale,
            &mut unwind,
        );
        self.layout[index].rect = transform.rect(surface);
        self.open_state(surface, interaction.as_ref(), &mut unwind);
        let state_material = Self::state_material(surface, interaction.as_ref(), surfaces.stretch);
        if let Some(material) = state_material {
            material.paint(self.canvas.commands(), &mut unwind);
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
        let (mut clips, groups, transform) = (painted.clips, painted.groups, painted.transform);
        self.paint_state_tint(decorated_surface, interaction.as_ref());
        let s = self.layout[index].innermost_layer();
        let rect = Rect::new(painted.pos.x, painted.pos.y, s.width, s.height);
        self.register_hit(index, &clips);
        let scroll = self.scroll_content(index, Playback::in_slot_if(groups.is_empty()));
        if scroll.is_some() {
            let viewport = surfaces.rect(rect);
            self.clip_scope(
                viewport,
                Corners::default(),
                transform,
                &mut clips,
                &mut unwind,
            );
        }
        let outer = (!groups.is_empty()).then(|| self.open_items());
        self.record_accessibility_geometry(index, rect, transform, &clips);
        let pushed = (interaction.as_ref().and_then(|state| state.content))
            .map(|content| inherited.content_colors.pushed(content));
        let inside = Inherited {
            clips: &clips,
            content_colors: pushed.as_ref().unwrap_or(inherited.content_colors),
            source: inherited.source,
            laid_out: (inherited.laid_out)
                .offset(painted.offset_at_rest.x, painted.offset_at_rest.y),
        };
        self.paint_kind(index, rect, transform, inside, text);
        if self.is_focused(id) && self.focus_shows_on(index) {
            self.paint_focus_ring(index, decorated_surface, interaction.as_ref());
        }
        let shift = scroll.as_ref().map_or(Point::default(), Scrolled::resting);
        let content = scroll.map(|scroll| self.open_content(index, scroll, transform));
        let inner = content.as_ref().map_or(transform, |(_, inner)| *inner);
        let start = Point::new(rect.x, rect.y);
        let children = Inherited {
            laid_out: inside.laid_out.offset(shift.x, shift.y),
            ..inside
        };
        self.paint_children(index, start, inner, children, text);
        if let Some((open, _)) = content {
            self.close_group(open);
        }
        if let Some(outer) = outer {
            let content = self.close_items(outer);
            self.paint_groups(index, groups, transform, content);
        }
        if self.shows_focus_within(index) {
            self.paint_focus_within(index, decorated_surface);
        }
        self.paint_state_outline(decorated_surface, interaction.as_ref());
        self.unwind(unwind);
        if let Some(open) = presence {
            self.close_group(open);
        }
    }

    /// Opens the presence group for the animated visual of the node at `index`, if the node has one. Returns that
    /// visual with the group and the places where the node and its decorations paint inside the group.
    fn open_visual(
        &mut self,
        index: usize,
        plan: &PaintPlan,
        parent_transform: Transform,
    ) -> OpenVisual {
        let id = self.nodes[index].id;
        let visual = self.visual(
            index,
            plan.space.relative(plan.target_rect),
            plan.visual_playback,
        );
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
        let _geometry_of_the_previous_frame = self.accessibility_geometry.insert(id, geometry);
    }

    /// Where the node paints, and how its motions play, before its groups open.
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
        // Decorations paint at the position without the animated offsets, because they paint inside the groups that
        // move the node by those offsets.
        let unmoved = at(placed);
        // The material group of the parent wraps an island at the bounds where the island paints. The motions of an
        // island therefore play by painting again, so that the group gets the bounds of each frame.
        let island =
            (node.modifier.iter()).any(|element| matches!(element, ModifierElement::Island(_)));
        let decorated = interactive_look
            || node.interaction.focus.is_some() && self.is_focused(id)
            || node.focus_within.is_some();
        let offsets = Playback::in_slot_if(node.transition.is_none() && !decorated && !island);
        let visual_playback = Playback::in_slot_if(!island);
        let space = match node.identity {
            Identity::InView => {
                let resting = self.moving.resting().then(parent_transform);
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

/// The visual of a node and the group that opens for it, with the places where the node and its decorations paint.
struct OpenVisual {
    visual: Option<AnimatedVisual>,
    presence: Option<OpenGroup>,
    transform: Transform,
    surfaces: Surfaces,
    surface: Rect,
    decorated_surface: Rect,
    radius_scale: Transform,
}

/// Where a node paints and how its motions play, before its groups open.
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
            self.hits.clear();
        }
        if node.enabled && (node.handles_input() || node.modal) {
            let rect = self.layout[index].rect;
            self.hits.push(Hit {
                action: node.action.clone(),
                activate: node.activate.clone(),
                context: node.context.clone(),
                pointer: node.pointer.clone(),
                drag: node.drag.clone(),
                transform: node.transform.clone(),
                group: self.moving.current,
                ..Hit::new(node.id, node.focusable && !node.modal, rect, clips.to_vec())
            });
        }
    }

    /// Whether the node `id` has the focus of a focused window.
    pub(super) fn is_focused(&self, id: NodeId) -> bool {
        self.window_focused && self.focus == Some(id)
    }

    /// Whether the node at `index` or a node inside it has the focus of a focused window, and the view shows that
    /// focus, such as after a move of the focus with the keyboard.
    fn shows_focus_within(&self, index: usize) -> bool {
        let focused =
            (self.focus.filter(|_| self.window_focused)).and_then(|id| index_of(&self.nodes, id));
        focused.is_some_and(|focused| {
            self.focus_shows_on(focused) && self.parents.ancestry(focused).any(|node| node == index)
        })
    }

    /// The focus ring of the interaction style, unless the state appearance paints an outline.
    fn paint_focus_ring(&mut self, index: usize, rect: Rect, state: Option<&AnimatedAppearance>) {
        let node = &self.nodes[index];
        let outlined = state.is_some_and(|appearance| appearance.outline.is_some());
        if !outlined && let Some(focus) = &node.interaction.focus {
            self.canvas.push(DrawCommand::Stroke {
                shape: Shape::rounded(rect, focus.radius),
                stroke: focus.stroke(),
            });
        }
    }

    fn paint_focus_within(&mut self, index: usize, rect: Rect) {
        if let Some(border) = &self.nodes[index].focus_within {
            self.canvas.push(DrawCommand::Stroke {
                shape: Shape::rounded(rect, border.radius),
                stroke: border.stroke(),
            });
        }
    }
}
