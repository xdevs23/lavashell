// SPDX-License-Identifier: GPL-3.0-only
use super::{
    DrawCommand, Drawing, FieldGeometry, Hit, Inherited, NodeKind, Point, Rect, TextMeasurer,
    TextStyle, Transform, View, field,
};
use crate::ui::IconStyle;

impl View {
    /// Paints the node at `index` in `rect` inside the clips of `inside`, with text and icons in the content colors
    /// of `inside`.
    pub(super) fn paint_kind(
        &mut self,
        index: usize,
        rect: Rect,
        transform: Transform,
        inside: Inherited,
        text: &mut impl TextMeasurer,
    ) {
        let colored = |color| inside.content_colors.applied_to(color);
        let clips = inside.clips;
        let node = &self.nodes[index];
        let id = node.id;
        match &node.kind {
            NodeKind::Text(value, style) => {
                text.observe_rendering();
                let focused = self.is_focused(id);
                if self.text_selectable(index) {
                    if node.enabled {
                        self.hits.push(Hit {
                            input: Some(index),
                            group: self.moving.current,
                            ..Hit::new(id, node.focusable, transform.rect(rect), clips.to_vec())
                        });
                    }
                    let (caret_width, device_pixel) = (self.caret_width(), self.device_pixel());
                    field::FieldPaint {
                        commands: self.canvas.commands(),
                        edits: &mut self.edits,
                        positions: &mut self.text_positions,
                        caret_width,
                        device_pixel,
                    }
                    .selectable_text(
                        node,
                        rect,
                        transform.scale,
                        focused,
                        text,
                    );
                } else if self.accessibility_text_geometry {
                    let geometry = text.geometry(value, style, rect.width);
                    let _geometry_of_the_previous_frame = self.text_positions.insert(
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
                    color: colored(style.color),
                    ..style.clone()
                };
                self.canvas.push(DrawCommand::Text {
                    text: value.clone(),
                    rect,
                    style,
                });
            }
            NodeKind::Icon {
                icon, color, style, ..
            } => {
                let filled = *style == IconStyle::Filled;
                icon.paint(rect, colored(*color), filled, self.canvas.commands());
            }
            NodeKind::Canvas {
                draw, draw_context, ..
            } => {
                text.observe_rendering();
                draw(&mut Drawing::new(
                    self.canvas.commands(),
                    rect,
                    draw_context,
                ));
            }
            NodeKind::TextInput { .. } => {
                let focused = self.is_focused(id);
                let allow_move = !self.protected_index(index)
                    && !self.accessibility_semantics(index).state.form.read_only;
                if node.enabled {
                    self.hits.push(Hit {
                        input: Some(index),
                        group: self.moving.current,
                        ..Hit::new(id, node.focusable, transform.rect(rect), clips.to_vec())
                    });
                }
                let (caret_width, device_pixel) = (self.caret_width(), self.device_pixel());
                field::FieldPaint {
                    commands: self.canvas.commands(),
                    edits: &mut self.edits,
                    positions: &mut self.text_positions,
                    caret_width,
                    device_pixel,
                }
                .paint(node, rect, transform.scale, focused, allow_move, text);
            }
            _ => {}
        }
    }
}
