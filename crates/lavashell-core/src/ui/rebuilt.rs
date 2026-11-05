// SPDX-License-Identifier: GPL-3.0-only
use super::{FieldContent, ModifierElement, Node, NodeKind};
use crate::rebuilt::BuiltLike;

impl Node {
    /// Whether this node in `tree` is known to lay out and paint like `earlier` in `earlier_tree`. Input handlers are
    /// left out, as the view reads them from the current nodes. The instance of a component is left out, as each
    /// composition creates a new one.
    pub(crate) fn is_built_like(
        &self,
        tree: &[Node],
        earlier: &Self,
        earlier_tree: &[Node],
    ) -> bool {
        let Self {
            id,
            kind,
            children,
            modifier,
            gap,
            align,
            flow,
            invalid_weight,
            visual_scale,
            scroll,
            activate: _,
            context: _,
            key_action: _,
            measured,
            content_sizes,
            modal,
            focusable,
            focus_within,
            tag,
            references,
            tooltip,
            interaction,
            interaction_observer,
            user_select,
            action: _,
            pointer: _,
            drag: _,
            transform: _,
            scroll_action: _,
            enabled,
            selected,
            transition,
            identity,
            semantics,
            semantics_provider,
            accessibility_actions: _,
            component,
            on_exit: _,
        } = self;
        let child_ids = (children.iter()).map(|child| tree[*child].id);
        let earlier_child_ids = (earlier.children.iter()).map(|child| earlier_tree[*child].id);
        *id == earlier.id
            && kind.is_built_like(&earlier.kind)
            && child_ids.eq(earlier_child_ids)
            && modifier.is_built_like(&earlier.modifier)
            && *gap == earlier.gap
            && *align == earlier.align
            && *flow == earlier.flow
            && *invalid_weight == earlier.invalid_weight
            && *visual_scale == earlier.visual_scale
            && scroll.is_built_like(&earlier.scroll)
            && self.handles_input() == earlier.handles_input()
            && measured.is_built_like(&earlier.measured)
            && content_sizes.is_built_like(&earlier.content_sizes)
            && *modal == earlier.modal
            && *focusable == earlier.focusable
            && *focus_within == earlier.focus_within
            && *tag == earlier.tag
            && *references == earlier.references
            && *tooltip == earlier.tooltip
            && *interaction == earlier.interaction
            && interaction_observer.is_built_like(&earlier.interaction_observer)
            && *user_select == earlier.user_select
            && *enabled == earlier.enabled
            && *selected == earlier.selected
            && *transition == earlier.transition
            && *identity == earlier.identity
            && *semantics == earlier.semantics
            && semantics_provider.is_built_like(&earlier.semantics_provider)
            && component.is_some() == earlier.component.is_some()
    }
}

impl BuiltLike for NodeKind {
    fn is_built_like(&self, earlier: &Self) -> bool {
        match self {
            Self::Container(arrangement) => {
                matches!(earlier, Self::Container(earlier) if arrangement == earlier)
            }
            Self::Text(text, style) => {
                matches!(earlier, Self::Text(earlier_text, earlier_style)
                    if text == earlier_text && style == earlier_style)
            }
            Self::Spacer(size) => matches!(earlier, Self::Spacer(earlier) if size == earlier),
            Self::Canvas {
                size,
                draw,
                draw_context,
            } => matches!(earlier, Self::Canvas {
                    size: earlier_size,
                    draw: earlier_draw,
                    draw_context: earlier_context,
                } if size == earlier_size && draw.is_built_like(earlier_draw) && draw_context == earlier_context),
            Self::Icon {
                icon,
                color,
                style,
                size,
            } => matches!(earlier, Self::Icon {
                    icon: earlier_icon,
                    color: earlier_color,
                    style: earlier_style,
                    size: earlier_size,
                } if icon == earlier_icon && color == earlier_color && style == earlier_style && size == earlier_size),
            Self::TextInput {
                content,
                placeholder,
                style,
                hints,
            } => matches!(earlier, Self::TextInput {
                    content: earlier_content,
                    placeholder: earlier_placeholder,
                    style: earlier_style,
                    hints: earlier_hints,
                } if content.is_built_like(earlier_content)
                    && placeholder == earlier_placeholder
                    && style == earlier_style
                    && hints == earlier_hints),
        }
    }
}

impl BuiltLike for FieldContent {
    fn is_built_like(&self, earlier: &Self) -> bool {
        match self {
            Self::Plain {
                value,
                change,
                lines,
            } => matches!(earlier, Self::Plain {
                    value: earlier_value,
                    change: earlier_change,
                    lines: earlier_lines,
                } if value == earlier_value && change.is_built_like(earlier_change) && lines == earlier_lines),
            Self::Protected(input) => {
                matches!(earlier, Self::Protected(earlier) if input.is_built_like(earlier))
            }
        }
    }
}

impl BuiltLike for ModifierElement {
    fn is_built_like(&self, earlier: &Self) -> bool {
        match self {
            Self::Padding(insets) => matches!(earlier, Self::Padding(earlier) if insets == earlier),
            Self::Width(width) => {
                matches!(earlier, Self::Width(earlier) if width.is_built_like(earlier))
            }
            Self::Height(height) => {
                matches!(earlier, Self::Height(earlier) if height.is_built_like(earlier))
            }
            Self::MinSize(size) => matches!(earlier, Self::MinSize(earlier) if size == earlier),
            Self::MaxSize(size) => matches!(earlier, Self::MaxSize(earlier) if size == earlier),
            Self::MeasurementLimit(size) => {
                matches!(earlier, Self::MeasurementLimit(earlier) if size == earlier)
            }
            Self::FillWidth => matches!(earlier, Self::FillWidth),
            Self::FillHeight => matches!(earlier, Self::FillHeight),
            Self::Background(brush) => {
                matches!(earlier, Self::Background(earlier) if brush == earlier)
            }
            Self::Border { stroke, corners } => matches!(earlier,
                Self::Border { stroke: earlier_stroke, corners: earlier_corners }
                    if stroke == earlier_stroke && corners == earlier_corners),
            Self::Clip(corners) => matches!(earlier, Self::Clip(earlier) if corners == earlier),
            Self::ClipRect(rect, corners) => matches!(earlier,
                Self::ClipRect(earlier_rect, earlier_corners) if rect == earlier_rect && corners == earlier_corners),
            Self::Effect { blur, filter } => matches!(earlier,
                Self::Effect { blur: earlier_blur, filter: earlier_filter }
                    if blur == earlier_blur && filter == earlier_filter),
            Self::Backdrop { effect, corners } => matches!(earlier,
                Self::Backdrop { effect: earlier_effect, corners: earlier_corners }
                    if effect == earlier_effect && corners == earlier_corners),
            Self::Shadow { shadow, corners } => matches!(earlier,
                Self::Shadow { shadow: earlier_shadow, corners: earlier_corners }
                    if shadow == earlier_shadow && corners == earlier_corners),
            Self::Outline {
                stroke,
                corners,
                offset,
            } => matches!(earlier,
                Self::Outline { stroke: earlier_stroke, corners: earlier_corners, offset: earlier_offset }
                    if stroke == earlier_stroke && corners == earlier_corners && offset == earlier_offset),
            Self::Material(material) => {
                matches!(earlier, Self::Material(earlier) if material == earlier)
            }
            Self::MaterialGroup { material, spacing } => matches!(earlier,
                Self::MaterialGroup { material: earlier_material, spacing: earlier_spacing }
                    if material == earlier_material && spacing == earlier_spacing),
            Self::Island(corners) => matches!(earlier, Self::Island(earlier) if corners == earlier),
            Self::Opacity(opacity) => {
                matches!(earlier, Self::Opacity(earlier) if opacity == earlier)
            }
            Self::Offset(offset) => {
                matches!(earlier, Self::Offset(earlier) if offset.is_built_like(earlier))
            }
            Self::Position(position) => {
                matches!(earlier, Self::Position(earlier) if position.is_built_like(earlier))
            }
            Self::ShownWhen(shown) => {
                matches!(earlier, Self::ShownWhen(earlier) if shown.is_built_like(earlier))
            }
            Self::AnimatedOffset(offset) => {
                matches!(earlier, Self::AnimatedOffset(earlier) if offset.is_built_like(earlier))
            }
            Self::AnimatedOpacity(opacity) => {
                matches!(earlier, Self::AnimatedOpacity(earlier) if opacity.is_built_like(earlier))
            }
            Self::AnimatedBackground(color) => {
                matches!(earlier, Self::AnimatedBackground(earlier) if color.is_built_like(earlier))
            }
            Self::FollowScroll { scroll, travel } => matches!(earlier,
                Self::FollowScroll { scroll: earlier_scroll, travel: earlier_travel }
                    if scroll.is_built_like(earlier_scroll) && travel == earlier_travel),
            Self::Invalid { value, cause } => matches!(earlier,
                Self::Invalid { value: earlier_value, cause: earlier_cause }
                    if value == earlier_value && cause == earlier_cause),
        }
    }
}
