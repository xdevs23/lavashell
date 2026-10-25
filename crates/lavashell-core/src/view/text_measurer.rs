// SPDX-License-Identifier: GPL-3.0-only
use super::{TextCaret, TextGeometry};
use crate::geometry::{Rect, Size};
use crate::scene::{ELLIPSIS, TextStyle};
use crate::units::Dp;

/// The width of a text on one unbroken line and the width of its longest word.
pub struct TextWidths {
    pub widest_line: Dp,
    pub longest_word: Dp,
}

/// The smallest width at which `text` draws correctly, by the rule described at
/// [`TextMeasurer::min_width`]. `widths` returns the widths of the string passed to it, which is
/// `text` or the ellipsis.
pub fn min_text_width(
    text: &str,
    style: &TextStyle<Dp>,
    mut widths: impl FnMut(&str) -> TextWidths,
) -> Dp {
    if style.max_lines == Some(1) {
        widths(ELLIPSIS).widest_line.min(widths(text).widest_line)
    } else {
        widths(text).longest_word
    }
}

/// Measures and lays out text. A width of infinity given to any method means that the width is
/// unbounded, so the text does not wrap.
pub trait TextMeasurer {
    /// Called before a text node or a canvas node is painted. A measurer reads the text settings of
    /// the renderer here as a reactive read, so a change of those settings paints the node again
    /// even when the measured size of the node does not change.
    fn observe_rendering(&mut self) {}

    fn measure(&mut self, text: &str, style: &TextStyle<Dp>, max_width: Dp) -> Size;

    /// The height of one line of text in `style`, which is the height of empty text.
    fn line_height(&mut self, style: &TextStyle<Dp>) -> Dp {
        self.measure("", style, Dp::INFINITY).height
    }

    /// The smallest width at which `text` draws correctly. Text limited to one line is cut with an
    /// ellipsis, so its smallest width is the width of the ellipsis, or the width of the whole text
    /// where the text is narrower. Text that wraps is as wide as its longest word.
    fn min_width(&mut self, text: &str, style: &TextStyle<Dp>) -> Dp {
        min_text_width(text, style, |text| TextWidths {
            widest_line: self.measure(text, style, Dp::INFINITY).width,
            longest_word: self.measure(text, style, Dp::ZERO).width,
        })
    }

    /// Measures protected text. The default measures the masked form of `text`, so a measurer that
    /// cannot shape private text never sees the plain characters.
    fn private_measure(
        &mut self,
        text: &crate::protected_text::PrivateText,
        style: &TextStyle<Dp>,
        max_width: Dp,
    ) -> Size {
        self.measure(&text.masked(), style, max_width)
    }

    /// The smallest width of protected text. Like [`TextMeasurer::private_measure`], the default
    /// uses the masked form of `text`.
    fn private_min_width(
        &mut self,
        text: &crate::protected_text::PrivateText,
        style: &TextStyle<Dp>,
    ) -> Dp {
        self.min_width(&text.masked(), style)
    }

    /// The caret geometry of protected text. Like [`TextMeasurer::private_measure`], the default
    /// uses the masked form of `text`.
    fn private_geometry(
        &mut self,
        text: &crate::protected_text::PrivateText,
        style: &TextStyle<Dp>,
        width: Dp,
    ) -> TextGeometry {
        self.geometry(&text.masked(), style, width)
    }

    fn positions(&mut self, text: &str, style: &TextStyle<Dp>) -> Vec<(usize, Dp)> {
        text.char_indices()
            .map(|(index, _)| index)
            .chain(std::iter::once(text.len()))
            .map(|index| {
                (
                    index,
                    self.measure(&text[..index], style, Dp::INFINITY).width,
                )
            })
            .collect()
    }

    fn geometry(&mut self, text: &str, style: &TextStyle<Dp>, width: Dp) -> TextGeometry {
        let height = self.line_height(style);
        let mut carets = Vec::new();
        let mut y = Dp::ZERO;
        let mut base = 0;
        let mut row = 0;
        for line in text.split('\n') {
            let mut shift = Dp::ZERO;
            for (index, x) in self.positions(line, style) {
                if x - shift > width && width > Dp::ZERO {
                    y += height;
                    row += 1;
                    shift = x;
                }
                carets.push(TextCaret {
                    index: base + index,
                    rect: Rect::new(x - shift, y, Dp::ZERO, height),
                    line: row,
                });
            }
            base += line.len() + 1;
            y += height;
            row += 1;
        }
        TextGeometry {
            carets,
            size: Size::new(width, y),
            ..Default::default()
        }
    }
}
