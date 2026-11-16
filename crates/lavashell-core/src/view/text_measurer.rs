// SPDX-License-Identifier: GPL-3.0-only
use super::{TextCaret, TextGeometry};
use crate::geometry::{Rect, Size};
use crate::scene::{ELLIPSIS, TextStyle};
use crate::units::Dp;

pub struct TextWidths {
    pub widest_line: Dp,
    pub longest_word: Dp,
}

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

pub trait TextMeasurer {
    fn observe_rendering(&mut self) {}

    fn measure(&mut self, text: &str, style: &TextStyle<Dp>, max_width: Dp) -> Size;

    fn line_height(&mut self, style: &TextStyle<Dp>) -> Dp {
        self.measure("", style, Dp::INFINITY).height
    }

    fn min_width(&mut self, text: &str, style: &TextStyle<Dp>) -> Dp {
        min_text_width(text, style, |text| TextWidths {
            widest_line: self.measure(text, style, Dp::INFINITY).width,
            longest_word: self.measure(text, style, Dp::ZERO).width,
        })
    }

    fn private_measure(
        &mut self,
        text: &crate::protected_text::PrivateText,
        style: &TextStyle<Dp>,
        max_width: Dp,
    ) -> Size {
        self.measure(&text.masked(), style, max_width)
    }

    fn private_min_width(
        &mut self,
        text: &crate::protected_text::PrivateText,
        style: &TextStyle<Dp>,
    ) -> Dp {
        self.min_width(&text.masked(), style)
    }

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
