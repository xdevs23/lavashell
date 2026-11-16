// SPDX-License-Identifier: GPL-3.0-only
use super::input_method::Sessions;
use super::{TextGeometry, TextMeasurer};
use crate::geometry::{Color, Point, Rect, Scale};
use crate::input_method::TextSender;
use crate::protected_text::{PrivateText, ProtectedInput};
use crate::scene::{DrawCommand, TextStyle};
use crate::shape::Corners;
use crate::text_edit::{FieldGeometry, TextEdit};
use crate::ui::{Lines, Node, NodeId, NodeKind};
use crate::units::Dp;
use std::collections::BTreeMap;

pub(super) struct TextEditing {
    pub edits: BTreeMap<NodeId, TextEdit>,
    pub positions: BTreeMap<NodeId, FieldGeometry>,
    pub sessions: Sessions,
    pub sender: TextSender,
}

impl Default for TextEditing {
    fn default() -> Self {
        Self {
            edits: BTreeMap::new(),
            positions: BTreeMap::new(),
            sessions: Sessions::default(),
            sender: TextSender::new(),
        }
    }
}

impl TextEditing {
    pub fn field_painter<'a>(
        &'a mut self,
        commands: &'a mut Vec<DrawCommand>,
        caret_width: Dp,
        device_pixel: Dp,
    ) -> FieldPaint<'a> {
        FieldPaint {
            commands,
            edits: &mut self.edits,
            positions: &mut self.positions,
            caret_width,
            device_pixel,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum FieldFocus {
    Focused,
    Unfocused,
}

#[derive(Clone, Copy)]
pub(super) enum TextMove {
    Allowed,
    Forbidden,
}

pub(super) struct FieldPaint<'a> {
    commands: &'a mut Vec<DrawCommand>,
    edits: &'a mut BTreeMap<NodeId, TextEdit>,
    positions: &'a mut BTreeMap<NodeId, FieldGeometry>,
    caret_width: Dp,
    device_pixel: Dp,
}

#[derive(Clone, Copy)]
enum LineLayout {
    One { caret_width: Dp },
    Several,
}

impl LineLayout {
    fn shaped(
        self,
        text: &mut impl TextMeasurer,
        value: &str,
        revealed: Option<&PrivateText>,
        style: &TextStyle<Dp>,
        rect: Rect,
    ) -> TextGeometry {
        let mut geometry = if let Some(private) = revealed {
            let unwrapped = || text.private_measure(private, style, Dp::INFINITY).width;
            let width = self.wrap_width(rect, unwrapped);
            text.private_geometry(private, style, width)
        } else {
            let unwrapped = || text.measure(value, style, Dp::INFINITY).width;
            let width = self.wrap_width(rect, unwrapped);
            text.geometry(value, style, width)
        };
        self.place(&mut geometry, rect);
        geometry
    }

    fn of(lines: Lines, caret_width: Dp) -> Self {
        match lines {
            Lines::One => Self::One { caret_width },
            Lines::Several => Self::Several,
        }
    }

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

    fn scroll(
        self,
        editing: &mut TextEdit,
        geometry: &TextGeometry,
        caret: Rect,
        extent: Dp,
        rect: Rect,
        focus: FieldFocus,
    ) -> Point {
        match self {
            Self::Several => {
                editing.scroll = Dp::ZERO;
                editing.scroll_y =
                    (editing.scroll_y).min((geometry.size.height - rect.height).max(Dp::ZERO));
                if focus == FieldFocus::Focused {
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
                if focus == FieldFocus::Focused {
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
        focus: FieldFocus,
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
        if focus == FieldFocus::Focused
            && let Some(color) = node.interaction.selection
        {
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
        focus: FieldFocus,
        text_move: TextMove,
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
        let geometry = lines.shaped(text, value, revealed.as_ref(), style, rect);
        let editing = self
            .edits
            .entry(node.id)
            .or_insert_with(|| TextEdit::at_end(value));
        editing.clamp_shaped(value, &geometry);
        let dropping = match text_move {
            TextMove::Allowed => editing.drop_caret(),
            TextMove::Forbidden => None,
        };
        let caret = dropping
            .unwrap_or_else(|| geometry.caret_on_line(editing.cursor, editing.line))
            .rect;
        let extent = (geometry.carets.iter())
            .map(|c| c.rect.x)
            .fold(Dp::ZERO, Dp::max);
        let scroll = lines.scroll(editing, &geometry, caret, extent, rect, focus);
        let selection = geometry.selection(editing.selection());
        self.commands.push(DrawCommand::Save);
        self.commands.push(DrawCommand::Clip {
            rect,
            corners: Corners::default(),
        });
        if focus == FieldFocus::Focused {
            self.paint_selection_and_caret(node, &selection, caret, rect, scroll);
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

    fn paint_selection_and_caret(
        &mut self,
        node: &Node,
        selection: &[Rect],
        caret: Rect,
        rect: Rect,
        scroll: Point,
    ) {
        if let Some(color) = node.interaction.selection {
            self.highlight(selection, color, rect, scroll);
        }
        if let Some(color) = node.interaction.caret {
            let bar = Rect::new(caret.x, caret.y, self.caret_width, caret.height);
            self.highlight(&[bar], color, rect, scroll);
        }
    }

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
