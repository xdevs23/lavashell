// SPDX-License-Identifier: GPL-3.0-only
use super::Inherited;
use crate::geometry::{Point, Rect};
use crate::scene::{DrawCommand, Drawing, TextStyle, Transform};
use crate::text_edit::FieldGeometry;
use crate::ui::{NodeId, NodeKind};
use crate::view::field::{FieldFocus, TextMove};
use crate::view::{Hit, HitClip, TextMeasurer, TextTarget, View};

impl View {
    pub(super) fn paint_kind(
        &mut self,
        index: usize,
        bounds: Rect,
        rect: Rect,
        transform: Transform,
        inside: Inherited,
        text: &mut impl TextMeasurer,
    ) {
        let colored = |color| inside.content_colors.applied_to(color);
        match &self.nodes[index].kind {
            NodeKind::Text(..) => self.paint_text(index, rect, transform, &inside, text),
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
        text: &mut impl TextMeasurer,
    ) {
        let node = &self.nodes[index];
        let NodeKind::Text(value, style) = &node.kind else {
            return;
        };
        let id = node.id;
        text.observe_rendering();
        let focus = self.field_focus(id);
        if self.text_selectable(index) {
            if node.enabled {
                let content = transform.rect(rect);
                self.routing.hits.push(Hit {
                    input: Some(TextTarget::new(index, content)),
                    group: self.routing.moving.current,
                    ..Hit::new(id, node.focusable, content, inside.clips.to_vec())
                });
            }
            let (caret_width, device_pixel) = (self.caret_width(), self.device_pixel());
            (self.editing)
                .field_painter(self.painting.canvas.commands(), caret_width, device_pixel)
                .selectable_text(node, rect, transform.scale, focus, text);
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
            self.routing.hits.push(Hit {
                input: Some(TextTarget::new(index, transform.rect(rect))),
                group: self.routing.moving.current,
                ..Hit::new(id, node.focusable, transform.rect(bounds), clips.to_vec())
            });
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
