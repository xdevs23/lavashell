// SPDX-License-Identifier: GPL-3.0-only
use super::{
    BTreeMap, Color, Corners, Dp, DrawCommand, FieldGeometry, Node, NodeId, NodeKind, Point, Rect,
    Scale, TextEdit, TextGeometry, TextMeasurer,
};
use crate::protected_text::ProtectedInput;
use crate::ui::Lines;

pub(super) struct FieldPaint<'a> {
    pub commands: &'a mut Vec<DrawCommand>,
    pub edits: &'a mut BTreeMap<NodeId, TextEdit>,
    pub positions: &'a mut BTreeMap<NodeId, FieldGeometry>,
    pub caret_width: Dp,
    /// The length of one device pixel. `highlight` draws each selection rectangle at least this wide, so that a
    /// selection of zero width still shows.
    pub device_pixel: Dp,
}

/// How a text field arranges its text.
#[derive(Clone, Copy)]
enum LineLayout {
    /// One line that scrolls sideways. The line is centered in the height of the field, and the field leaves room
    /// after the text for a caret `caret_width` wide.
    One { caret_width: Dp },
    /// Lines wrapped at the width of the field, which scroll up and down.
    Several,
}

impl LineLayout {
    fn of(lines: Lines, caret_width: Dp) -> Self {
        match lines {
            Lines::One => Self::One { caret_width },
            Lines::Several => Self::Several,
        }
    }

    /// The width at which the field lays out its text. For one line, this is the width of the field minus the width
    /// of the caret, or the width of the `unwrapped` text where that text is wider. The line therefore never wraps,
    /// and a short text is aligned within the width of the field.
    fn wrap_width(self, rect: Rect, unwrapped: impl FnOnce() -> Dp) -> Dp {
        match self {
            Self::One { caret_width } => (rect.width - caret_width).max(unwrapped()),
            Self::Several => rect.width,
        }
    }

    fn place(self, geometry: &mut TextGeometry, rect: Rect) {
        if let Self::One { .. } = self {
            let above_centered_text = (rect.height - geometry.size.height).max(Dp::ZERO) / 2.0;
            for caret in &mut geometry.carets {
                caret.rect.y += above_centered_text;
            }
            for cluster in &mut geometry.clusters {
                cluster.bounds.y += above_centered_text;
            }
        }
    }

    /// Limits the scroll offset of `editing` to the part of the text that overflows the field, and scrolls the `caret`
    /// into view when the field is `focused`. Returns the new scroll offset. `extent` is the position of the caret
    /// furthest to the right.
    fn scroll(
        self,
        editing: &mut TextEdit,
        geometry: &TextGeometry,
        caret: Rect,
        extent: Dp,
        rect: Rect,
        focused: bool,
    ) -> Point {
        match self {
            Self::Several => {
                editing.scroll = Dp::ZERO;
                editing.scroll_y =
                    (editing.scroll_y).min((geometry.size.height - rect.height).max(Dp::ZERO));
                if focused {
                    if caret.y < editing.scroll_y {
                        editing.scroll_y = caret.y;
                    }
                    if caret.y + caret.height > editing.scroll_y + rect.height {
                        editing.scroll_y = (caret.y + caret.height - rect.height).max(Dp::ZERO);
                    }
                }
            }
            Self::One { caret_width } => {
                editing.scroll_y = Dp::ZERO;
                editing.scroll =
                    (editing.scroll).min((extent - rect.width + caret_width).max(Dp::ZERO));
                if focused {
                    if caret.x < editing.scroll {
                        editing.scroll = caret.x;
                    }
                    if caret.x > editing.scroll + rect.width - caret_width {
                        editing.scroll = (caret.x - rect.width + caret_width).max(Dp::ZERO);
                    }
                }
            }
        }
        Point::new(editing.scroll, editing.scroll_y)
    }

    fn text_rect(self, rect: Rect, scroll: Point, extent: Dp, geometry: &TextGeometry) -> Rect {
        let (width, height) = match self {
            Self::One { caret_width } => (
                extent.max(rect.width) + caret_width,
                geometry.size.height.max(rect.height),
            ),
            Self::Several => (rect.width, geometry.size.height),
        };
        Rect::new(rect.x - scroll.x, rect.y - scroll.y, width, height)
    }
}

impl FieldPaint<'_> {
    pub fn selectable_text(
        &mut self,
        node: &Node,
        rect: Rect,
        scale: Scale,
        focused: bool,
        text: &mut impl TextMeasurer,
    ) {
        let NodeKind::Text(value, style) = &node.kind else {
            return;
        };
        let geometry = text.geometry(value, style, rect.width);
        let above_centered_text = (rect.height - geometry.size.height).max(Dp::ZERO) / 2.0;
        let scroll = Point::new(Dp::ZERO, -above_centered_text);
        let editing = self
            .edits
            .entry(node.id)
            .or_insert_with(|| TextEdit::at_end(value));
        editing.clamp_shaped(value, &geometry);
        if focused && let Some(color) = node.interaction.selection {
            let selection = geometry.selection(editing.selection());
            self.highlight(&selection, color, rect, scroll);
        }
        let _geometry_of_the_previous_frame = self.positions.insert(
            node.id,
            FieldGeometry {
                value: value.clone(),
                geometry,
                scroll,
                scale,
            },
        );
    }

    pub fn paint(
        &mut self,
        node: &Node,
        rect: Rect,
        scale: Scale,
        focused: bool,
        allow_move: bool,
        text: &mut impl TextMeasurer,
    ) {
        let NodeKind::TextInput {
            content,
            placeholder,
            style,
            ..
        } = &node.kind
        else {
            return;
        };
        let style = &content.line_style(style);
        let lines = LineLayout::of(content.lines(), self.caret_width);
        let value = &*content.shown();
        let revealed = content.protected().and_then(ProtectedInput::presentation);
        let mut geometry = if let Some(private) = &revealed {
            let unwrapped = || text.private_measure(private, style, Dp::INFINITY).width;
            let width = lines.wrap_width(rect, unwrapped);
            text.private_geometry(private, style, width)
        } else {
            let unwrapped = || text.measure(value, style, Dp::INFINITY).width;
            let width = lines.wrap_width(rect, unwrapped);
            text.geometry(value, style, width)
        };
        lines.place(&mut geometry, rect);
        let editing = self
            .edits
            .entry(node.id)
            .or_insert_with(|| TextEdit::at_end(value));
        editing.clamp_shaped(value, &geometry);
        let dropping = allow_move.then(|| editing.drop_caret()).flatten();
        let caret = dropping
            .unwrap_or_else(|| geometry.caret_on_line(editing.cursor, editing.line))
            .rect;
        let extent = (geometry.carets.iter())
            .map(|c| c.rect.x)
            .fold(Dp::ZERO, Dp::max);
        let scroll = lines.scroll(editing, &geometry, caret, extent, rect, focused);
        let selection = geometry.selection(editing.selection());
        self.commands.push(DrawCommand::Save);
        self.commands.push(DrawCommand::Clip {
            rect,
            corners: Corners::default(),
        });
        if focused {
            if let Some(color) = node.interaction.selection {
                self.highlight(&selection, color, rect, scroll);
            }
            if let Some(color) = node.interaction.caret {
                let bar = Rect::new(caret.x, caret.y, self.caret_width, caret.height);
                self.highlight(&[bar], color, rect, scroll);
            }
        }
        let mut styled = style.clone();
        if value.is_empty()
            && let Some(color) = node.interaction.placeholder
        {
            styled.color = color;
        }
        let text_rect = lines.text_rect(rect, scroll, extent, &geometry);
        self.commands.push(match revealed {
            Some(private) => DrawCommand::PrivateText {
                text: private,
                rect: text_rect,
                style: styled,
            },
            None => DrawCommand::Text {
                text: if value.is_empty() { placeholder } else { value }.to_string(),
                rect: text_rect,
                style: styled,
            },
        });
        self.commands.push(DrawCommand::Restore);
        let _geometry_of_the_previous_frame = self.positions.insert(
            node.id,
            FieldGeometry {
                value: value.to_string(),
                geometry,
                scroll,
                scale,
            },
        );
    }

    /// Fills each of `rects` with `color`, at least one device pixel wide. `rects` are in the coordinates of the text,
    /// which the field at `rect` shows scrolled by `scroll`.
    fn highlight(&mut self, rects: &[Rect], color: Color, rect: Rect, scroll: Point) {
        for r in rects {
            self.commands.push(DrawCommand::Fill {
                shape: Rect::new(
                    rect.x + r.x - scroll.x,
                    rect.y + r.y - scroll.y,
                    r.width.max(self.device_pixel),
                    r.height,
                )
                .into(),
                brush: color.into(),
            });
        }
    }
}
