// SPDX-License-Identifier: GPL-3.0-only
//! Editing of UTF-8 text with the carets, clusters and lines measured by text shaping. Every position in the text
//! is a byte offset on a character boundary.

use crate::accessibility::TextDirection;
use crate::geometry::{Point, Rect, Scale, Size};
use crate::keyboard::Key;
use crate::units::Dp;
use std::ops::Range;

mod drag;

use drag::SelectionDrag;

#[cfg(test)]
mod drag_tests;

#[derive(Clone, Copy, Debug)]
pub struct TextCaret {
    pub index: usize,
    pub rect: Rect,
    /// The visual line the caret is on, counted from the first line of the text.
    pub line: usize,
}

#[derive(Clone, Debug)]
pub struct TextCluster {
    /// The byte offsets in the UTF-8 text where the cluster starts and ends, in the same units as the `index`
    /// of a `TextCaret`. Accessibility snapshots convert these offsets into counts of Unicode scalar values.
    pub byte_range: Range<usize>,
    pub bounds: Rect,
    pub direction: TextDirection,
    /// The visual line the cluster is on, counted from the first line of the text.
    pub line: usize,
}

#[derive(Clone, Debug, Default)]
pub struct TextGeometry {
    pub carets: Vec<TextCaret>,
    pub size: Size,
    pub clusters: Vec<TextCluster>,
    /// The byte range of each visual line in the UTF-8 text, in visual order. A line terminator is part of the
    /// range of its line. An empty vector means that the line ranges are not known. An empty range at the end
    /// stands for a blank last line.
    pub lines: Vec<std::ops::Range<usize>>,
    /// The byte offsets in the UTF-8 text where the shaper found the start of a word. `None` when the word starts
    /// are not known.
    pub word_starts: Option<Vec<usize>>,
}

impl TextGeometry {
    pub(crate) fn boundary(&self, index: usize) -> usize {
        self.carets
            .iter()
            .filter(|caret| caret.index <= index)
            .map(|caret| caret.index)
            .max()
            .unwrap_or(0)
    }

    /// The last caret at or before `index`. For text without carets, a caret of zero width at the start of the
    /// text.
    #[must_use]
    pub fn caret(&self, index: usize) -> TextCaret {
        (self.carets.iter().rev().find(|c| c.index <= index).copied()).unwrap_or(TextCaret {
            index: 0,
            rect: Rect::new(Dp::ZERO, Dp::ZERO, Dp::ZERO, self.size.height),
            line: 0,
        })
    }

    #[must_use]
    pub fn hit(&self, point: Point) -> usize {
        self.hit_caret(point).index
    }

    /// The caret at `index` on `line`, where the text has one there, or else the caret at
    /// `index`. A position where a line wraps has a caret at the end of one line and at the start
    /// of the next.
    #[must_use]
    pub fn caret_on_line(&self, index: usize, line: Option<usize>) -> TextCaret {
        (line.and_then(|line| (self.carets.iter()).find(|c| c.index == index && c.line == line)))
            .copied()
            .unwrap_or_else(|| self.caret(index))
    }

    #[must_use]
    pub fn hit_caret(&self, point: Point) -> TextCaret {
        self.carets
            .iter()
            .min_by(|a, b| {
                let distance = |c: &TextCaret| {
                    let middle = c.rect.y + c.rect.height / 2.0;
                    let row = if point.y >= c.rect.y && point.y < c.rect.y + c.rect.height {
                        Dp::ZERO
                    } else {
                        (point.y - middle).abs()
                    };
                    (row, (point.x - c.rect.x).abs())
                };
                let (ay, ax) = distance(a);
                let (by, bx) = distance(b);
                ay.total_cmp(&by).then(ax.total_cmp(&bx))
            })
            .copied()
            .unwrap_or_else(|| self.caret(0))
    }

    /// The rectangles that cover the text in `range`. Touching rectangles on one line merge into one.
    /// A selected line end gets a rectangle of zero width. The widget that draws the selection widens
    /// that rectangle to a visible width of its choice.
    #[must_use]
    pub fn selection(&self, range: Range<usize>) -> Vec<Rect> {
        if range.is_empty() {
            return Vec::new();
        }
        let mut pieces: Vec<(usize, Rect)> = if self.clusters.is_empty() {
            let pairs = self.carets.iter().zip(self.carets.iter().skip(1));
            pairs
                .filter(|(a, b)| a.index != b.index && range.contains(&a.index))
                .map(|(a, b)| {
                    let right = if b.line == a.line { b.rect.x } else { a.rect.x };
                    let left = a.rect.x.min(right);
                    let width = (right - a.rect.x).abs();
                    (a.line, Rect::new(left, a.rect.y, width, a.rect.height))
                })
                .collect()
        } else {
            (self.clusters.iter())
                .filter(|cluster| {
                    cluster.byte_range.start < range.end && cluster.byte_range.end > range.start
                })
                .map(|cluster| (cluster.line, cluster.bounds))
                .collect()
        };
        pieces.sort_by(|(a_line, a), (b_line, b)| a_line.cmp(b_line).then(a.x.total_cmp(&b.x)));
        let mut merged: Vec<(usize, Rect)> = Vec::new();
        for (line, rect) in pieces {
            match merged.last_mut() {
                Some((last_line, last)) if *last_line == line && touch(*last, rect) => {
                    *last = last.union(rect);
                }
                _ => merged.push((line, rect)),
            }
        }
        merged.into_iter().map(|(_, rect)| rect).collect()
    }
}

fn touch(a: Rect, b: Rect) -> bool {
    b.x <= a.x + a.width && b.x + b.width >= a.x
}

pub(crate) struct FieldGeometry {
    pub value: String,
    pub geometry: TextGeometry,
    pub scroll: Point,
    pub scale: Scale,
}

impl FieldGeometry {
    /// Whether this geometry was shaped for `value` and has a caret at both ends of `range`.
    pub fn shaped_range(&self, value: &str, range: &Range<usize>) -> bool {
        self.value == value
            && [range.start, range.end].iter().all(|&byte| {
                value.is_char_boundary(byte)
                    && self.geometry.carets.iter().any(|caret| caret.index == byte)
            })
    }
}

#[derive(Clone, Default)]
pub(crate) struct TextEdit {
    pub cursor: usize,
    pub anchor: Option<usize>,
    pub scroll: Dp,
    pub scroll_y: Dp,
    pub goal_x: Option<Dp>,
    /// The line on which the caret is shown when the caret index lies at a wrap, where it is both the
    /// end of one line and the start of the next.
    pub line: Option<usize>,
    drag: Option<SelectionDrag>,
}

impl TextEdit {
    pub fn at_end(value: &str) -> Self {
        Self {
            cursor: value.len(),
            anchor: None,
            scroll: Dp::ZERO,
            scroll_y: Dp::ZERO,
            goal_x: None,
            line: None,
            drag: None,
        }
    }

    pub fn clamp(&mut self, value: &str) {
        self.cursor = boundary(value, self.cursor);
        self.anchor = self.anchor.map(|anchor| boundary(value, anchor));
    }

    pub fn selection(&self) -> Range<usize> {
        let anchor = self.anchor.unwrap_or(self.cursor);
        self.cursor.min(anchor)..self.cursor.max(anchor)
    }

    /// The part of `value` before the selection, or before the cursor when nothing is selected.
    pub fn before_selection<'a>(&self, value: &'a str) -> &'a str {
        &value[..boundary(value, self.selection().start)]
    }

    pub fn replace(&mut self, value: &mut String, text: &str) {
        self.clamp(value);
        let range = self.selection();
        self.cursor = range.start + text.len();
        self.anchor = None;
        self.goal_x = None;
        self.line = None;
        value.replace_range(range, text);
    }

    pub fn select_all(&mut self, value: &str) {
        self.anchor = Some(0);
        self.cursor = value.len();
        self.line = None;
        self.goal_x = None;
    }

    pub fn key(&mut self, value: &mut String, key: Key, shift: bool) {
        self.clamp(value);
        let selection = self.selection();
        if matches!(key, Key::Backspace | Key::Delete) {
            if selection.is_empty() {
                self.anchor = Some(match key {
                    Key::Backspace => previous(value, self.cursor),
                    _ => next(value, self.cursor),
                });
            }
            self.replace(value, "");
            return;
        }
        let destination = match key {
            Key::Left if !shift && !selection.is_empty() => selection.start,
            Key::Right if !shift && !selection.is_empty() => selection.end,
            Key::Left => previous(value, self.cursor),
            Key::Right => next(value, self.cursor),
            Key::Home => 0,
            Key::End => value.len(),
            _ => return,
        };
        self.move_to(destination, shift);
        self.goal_x = None;
        self.line = None;
    }

    pub fn move_to(&mut self, destination: usize, shift: bool) {
        if shift {
            self.anchor = self.anchor.or(Some(self.cursor));
        } else {
            self.anchor = None;
        }
        self.cursor = destination;
    }

    pub fn shaped_key(
        &mut self,
        value: &mut String,
        key: Key,
        shift: bool,
        geometry: &TextGeometry,
    ) {
        self.clamp(value);
        let selection = self.selection();
        if matches!(key, Key::Left | Key::Right | Key::Backspace | Key::Delete)
            && selection.is_empty()
        {
            let target = if matches!(key, Key::Left | Key::Backspace) {
                geometry
                    .carets
                    .iter()
                    .rev()
                    .find(|c| c.index < self.cursor)
                    .map_or(0, |c| c.index)
            } else {
                geometry
                    .carets
                    .iter()
                    .find(|c| c.index > self.cursor)
                    .map_or(value.len(), |c| c.index)
            };
            if matches!(key, Key::Backspace | Key::Delete) {
                self.anchor = Some(target);
                self.replace(value, "");
            } else {
                self.move_to(target, shift);
                self.goal_x = None;
                self.line = None;
            }
            return;
        }
        let current = geometry.caret_on_line(self.cursor, self.line);
        let target = match key {
            Key::Up | Key::Down => {
                let x = *self.goal_x.get_or_insert(current.rect.x);
                let height = current.rect.height;
                let middle_of_line = current.rect.y + height / 2.0;
                let middle_of_line_above = middle_of_line - height;
                let middle_of_line_below = middle_of_line + height;
                let y = if key == Key::Up {
                    middle_of_line_above
                } else {
                    middle_of_line_below
                };
                let caret = geometry.hit_caret(Point::new(x, y));
                self.line = Some(caret.line);
                caret.index
            }
            Key::Home | Key::End => {
                self.goal_x = None;
                self.line = Some(current.line);
                let mut row = (geometry.carets.iter()).filter(|c| c.line == current.line);
                if key == Key::Home {
                    row.next().map_or(0, |c| c.index)
                } else {
                    row.next_back().map_or(value.len(), |c| c.index)
                }
            }
            _ => return self.key(value, key, shift),
        };
        self.move_to(target, shift);
    }
}

fn boundary(value: &str, index: usize) -> usize {
    let mut index = index.min(value.len());
    while !value.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn previous(value: &str, cursor: usize) -> usize {
    value[..cursor]
        .char_indices()
        .next_back()
        .map_or(0, |(i, _)| i)
}

fn next(value: &str, cursor: usize) -> usize {
    cursor + value[cursor..].chars().next().map_or(0, char::len_utf8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selections_and_deletion_keep_utf8_boundaries() {
        let mut value = "a雪🙂z".to_string();
        let mut edit = TextEdit::at_end(&value);
        edit.key(&mut value, Key::Left, false);
        edit.key(&mut value, Key::Left, true);
        edit.replace(&mut value, "é");
        assert_eq!(value, "a雪éz");
        edit.key(&mut value, Key::Backspace, false);
        assert_eq!(value, "a雪z");
        edit.key(&mut value, Key::Delete, false);
        assert_eq!(value, "a雪");
        edit.select_all(&value);
        edit.replace(&mut value, "new");
        assert_eq!(value, "new");
    }
}
