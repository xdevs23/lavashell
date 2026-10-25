// SPDX-License-Identifier: GPL-3.0-only
use super::{Dp, DrawCommand, Rect, Shape, TextMeasurer, View};

impl View {
    pub(in crate::view) fn paint_tooltip(&mut self, text: &mut impl TextMeasurer) {
        let hovered = self.primary().and_then(|primary| primary.over);
        let Some((index, tooltip)) = self.nodes.iter().enumerate().find_map(|(index, node)| {
            let tooltip = node.tooltip.as_ref().filter(|_| Some(node.id) == hovered)?;
            Some((index, tooltip))
        }) else {
            return;
        };
        let Some(decoration) = self.nodes[index].interaction.tooltip.as_ref() else {
            return;
        };
        let label = &tooltip.label;
        let style = decoration.text.clone().resolve(&tooltip.text_scaling);
        // The tooltip is as far from the sides of the view as from the control it describes.
        let (left, right) = (decoration.gap, self.size.width - decoration.gap);
        let wrap = (right - left - decoration.padding.horizontal()).max(Dp::ZERO);
        let size = text.measure(label, &style, wrap);
        let control = self.layout[index].rect;
        let width = size.width + decoration.padding.horizontal();
        let height = size.height + decoration.padding.vertical();
        let centered_under_control = control.x + (control.width - width) / 2.0;
        let x = centered_under_control.min(right - width).max(left);
        let y = if control.y + control.height + decoration.gap + height > self.size.height {
            (control.y - height - decoration.gap).max(Dp::ZERO)
        } else {
            control.y + control.height + decoration.gap
        };
        let rect = Rect::new(x, y, width, height);
        self.canvas.push(DrawCommand::Fill {
            shape: Shape::rounded(rect, decoration.border.radius),
            brush: decoration.background.into(),
        });
        self.canvas.push(DrawCommand::Stroke {
            shape: Shape::rounded(rect, decoration.border.radius),
            stroke: decoration.border.stroke(),
        });
        self.canvas.push(DrawCommand::Text {
            text: label.clone(),
            rect: Rect::new(
                x + decoration.padding.left,
                y + decoration.padding.top,
                size.width,
                size.height,
            ),
            style,
        });
    }
}
