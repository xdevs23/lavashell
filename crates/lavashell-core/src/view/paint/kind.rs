// SPDX-License-Identifier: GPL-3.0-only
use super::Inherited;
use crate::geometry::{Point, Rect};
use crate::scene::{DrawCommand, Drawing, TextStyle, Transform};
use crate::text_edit::FieldGeometry;
use crate::ui::{NodeId, NodeKind};
use crate::units::Dp;
use crate::view::field::{FieldFocus, TextMove};
use crate::view::transition::TextBlend;
use crate::view::{Hit, HitClip, PaintedHit, TextMeasurer, TextTarget, View};

#[derive(Clone, Copy)]
pub(super) struct KindInput {
    pub bounds: Rect,
    pub rect: Rect,
    pub transform: Transform,
    pub text_blend: Option<TextBlend>,
}

impl View {
    pub(super) fn painted_rect(
        &self,
        index: usize,
        rect: Rect,
        text_blend: Option<TextBlend>,
        text: &mut impl TextMeasurer,
    ) -> Rect {
        match (&self.nodes[index].kind, text_blend) {
            (NodeKind::Text(value, style), Some(blend)) => {
                rect_keeping_lines(rect, value, style, &blend.applied_to(style), text)
            }
            _ => rect,
        }
    }

    pub(super) fn paint_kind(
        &mut self,
        index: usize,
        input: KindInput,
        inside: Inherited,
        text: &mut impl TextMeasurer,
    ) {
        let KindInput {
            bounds,
            rect,
            transform,
            text_blend,
        } = input;
        let colored = |color| inside.content_colors.applied_to(color);
        match &self.nodes[index].kind {
            NodeKind::Text(..) => {
                self.paint_text(index, rect, transform, &inside, text_blend, text);
            }
            NodeKind::Icon {
                icon, color, style, ..
            } => {
                icon.paint(
                    rect,
                    colored(*color),
                    *style,
                    self.painting.canvas.commands(),
                );
            }
            NodeKind::Canvas {
                draw, draw_context, ..
            } => {
                text.observe_rendering();
                draw(&mut Drawing::new(
                    self.painting.canvas.commands(),
                    rect,
                    draw_context,
                ));
            }
            NodeKind::TextInput { .. } => {
                self.paint_text_input(index, bounds, rect, transform, inside.clips, text);
            }
            NodeKind::Container(_) | NodeKind::Spacer(_) => {}
        }
    }

    fn paint_text(
        &mut self,
        index: usize,
        rect: Rect,
        transform: Transform,
        inside: &Inherited,
        text_blend: Option<TextBlend>,
        text: &mut impl TextMeasurer,
    ) {
        let node = &self.nodes[index];
        let NodeKind::Text(value, node_style) = &node.kind else {
            return;
        };
        let id = node.id;
        text.observe_rendering();
        let blended = text_blend.map(|blend| blend.applied_to(node_style));
        let style = blended.as_ref().unwrap_or(node_style);
        let focus = self.field_focus(id);
        if self.text_selectable(index) {
            if node.enabled {
                let content = transform.rect(rect);
                self.routing.painted_hits.push(PaintedHit::Input(Hit {
                    input: Some(TextTarget::new(index, content)),
                    ..Hit::new(id, node.focusable, self.hit_area(content, inside.clips))
                }));
            }
            let (caret_width, device_pixel) = (self.caret_width(), self.device_pixel());
            (self.editing)
                .field_painter(self.painting.canvas.commands(), caret_width, device_pixel)
                .selectable_text(node, style, rect, transform.scale, focus, text);
        } else if self.accessibility.text_geometry {
            let geometry = text.geometry(value, style, rect.width);
            let _geometry_of_the_previous_frame = self.editing.positions.insert(
                id,
                FieldGeometry {
                    value: value.clone(),
                    geometry,
                    scroll: Point::default(),
                    scale: transform.scale,
                },
            );
        }
        let style = TextStyle {
            color: inside.content_colors.applied_to(style.color),
            ..style.clone()
        };
        self.painting.canvas.push(DrawCommand::Text {
            text: value.clone(),
            rect,
            style,
        });
    }

    fn paint_text_input(
        &mut self,
        index: usize,
        bounds: Rect,
        rect: Rect,
        transform: Transform,
        clips: &[HitClip],
        text: &mut impl TextMeasurer,
    ) {
        let node = &self.nodes[index];
        let id = node.id;
        let focus = self.field_focus(id);
        let text_move = if self.protected_index(index)
            || self.accessibility_semantics(index).state.form.read_only
        {
            TextMove::Forbidden
        } else {
            TextMove::Allowed
        };
        if node.enabled {
            let area = self.hit_area(transform.rect(bounds), clips);
            self.routing.painted_hits.push(PaintedHit::Input(Hit {
                input: Some(TextTarget::new(index, transform.rect(rect))),
                ..Hit::new(id, node.focusable, area)
            }));
        }
        let (caret_width, device_pixel) = (self.caret_width(), self.device_pixel());
        (self.editing)
            .field_painter(self.painting.canvas.commands(), caret_width, device_pixel)
            .paint(node, rect, transform.scale, focus, text_move, text);
    }

    fn field_focus(&self, id: NodeId) -> FieldFocus {
        if self.is_focused(id) {
            FieldFocus::Focused
        } else {
            FieldFocus::Unfocused
        }
    }
}

fn rect_keeping_lines(
    rect: Rect,
    value: &str,
    laid_out: &TextStyle<Dp>,
    blended: &TextStyle<Dp>,
    text: &mut impl TextMeasurer,
) -> Rect {
    let laid_out_width = text.measure(value, laid_out, Dp::INFINITY).width;
    if laid_out_width <= Dp::ZERO {
        return rect;
    }
    let growth = text.measure(value, blended, Dp::INFINITY).width / laid_out_width;
    let added = (rect.width * growth - rect.width).max(Dp::ZERO);
    let left = added * (blended.alignment).share_of_widening_on_the_left(blended.direction);
    Rect::new(rect.x - left, rect.y, rect.width + added, rect.height)
}
