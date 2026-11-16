// SPDX-License-Identifier: GPL-3.0-only
use crate::board::{Grid, KeyAt, at_both_ends};
use crate::keyboard::Snapshot;
use crate::layouts::Layout;
use crate::press::PressSerial;
use lavashell_core::prelude::{Dp, Point, Rect, Size};
use std::iter::successors;

pub(crate) const ENTRY_GAP: Dp = Dp::new(4.0);
pub(crate) const ENTRY_WIDTH: Dp = Dp::new(80.0);
pub(crate) const STRIP_PADDING: Dp = Dp::new(6.0);
pub(crate) const LIST_WIDTH: Dp = Dp::new(340.0);
pub(crate) const LIST_PADDING: Dp = Dp::new(8.0);
pub(crate) const HEADING_HEIGHT: Dp = Dp::new(34.0);

const ABOVE: Dp = Dp::new(8.0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Offer {
    FurtherLetters(char),
    Layouts,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Popup {
    pub at: KeyAt,
    pub open: Open,
    pub chosen: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Open {
    Held { press: PressSerial, offer: Offer },
    LayoutsLeft,
}

impl Popup {
    pub fn new(press: PressSerial, at: KeyAt, offer: Offer) -> Self {
        Self {
            at,
            open: Open::Held { press, offer },
            chosen: match offer {
                Offer::FurtherLetters(_) => Some(0),
                Offer::Layouts => None,
            },
        }
    }

    pub fn offer(&self) -> Offer {
        match self.open {
            Open::Held { offer, .. } => offer,
            Open::LayoutsLeft => Offer::Layouts,
        }
    }

    pub fn under_finger(&self) -> bool {
        matches!(self.open, Open::Held { .. })
    }

    pub fn held_by(&self, press: PressSerial) -> bool {
        matches!(self.open, Open::Held { press: held, .. } if held == press)
    }
}

pub(crate) enum Entries {
    FurtherLetters(FurtherLetters),
    Layouts(Vec<&'static Layout>),
}

impl Entries {
    pub fn of(popup: &Popup, snapshot: &Snapshot) -> Option<Self> {
        match popup.offer() {
            Offer::FurtherLetters(letter) => {
                let letters = snapshot.further_letters(letter);
                (!letters.is_empty()).then_some(Self::FurtherLetters(letters))
            }
            Offer::Layouts => Some(Self::Layouts(snapshot.layouts.clone())),
        }
    }

    pub fn len(&self) -> usize {
        match self {
            Self::FurtherLetters(letters) => letters.len(),
            Self::Layouts(layouts) => layouts.len(),
        }
    }
}

pub(crate) struct FurtherLetters {
    letters: Vec<char>,
}

impl FurtherLetters {
    pub fn new(listed: impl IntoIterator<Item = char>) -> Self {
        let mut letters = Vec::new();
        for letter in listed {
            if !letters.contains(&letter) {
                letters.push(letter);
            }
        }
        Self { letters }
    }

    fn count(&self) -> f64 {
        self.letters.iter().map(|_| 1.0).sum()
    }

    pub fn get(&self, index: usize) -> Option<char> {
        self.letters.get(index).copied()
    }

    pub fn len(&self) -> usize {
        self.letters.len()
    }

    pub fn is_empty(&self) -> bool {
        self.letters.is_empty()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Rows {
    One,
    Two,
}

impl Rows {
    fn count(self) -> usize {
        match self {
            Self::One => 1,
            Self::Two => 2,
        }
    }

    fn columns(self, letters: f64) -> f64 {
        match self {
            Self::One => letters,
            Self::Two => (letters / 2.0).ceil(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Strip {
    columns: usize,
    rows: Rows,
    letter_width: Dp,
}

impl Strip {
    fn new(letters: &FurtherLetters, panel: Dp) -> Self {
        let count = letters.count();
        let rows = if strip_width(count, ENTRY_WIDTH) > panel {
            Rows::Two
        } else {
            Rows::One
        };
        let across = rows.columns(count);
        let room = (panel - strip_spacing(across)).max(Dp::ZERO) / across;
        Self {
            columns: letters.len().div_ceil(rows.count()),
            rows,
            letter_width: ENTRY_WIDTH.min(room),
        }
    }

    fn letters(self, letters: usize, height: Dp) -> Vec<Rect> {
        let row = |y: Dp| {
            let first = Rect::new(STRIP_PADDING, y, self.letter_width, height);
            let step = Point::new(self.letter_width + ENTRY_GAP, Dp::ZERO);
            successors(Some(first), move |rect| Some(rect.translated(step))).take(self.columns)
        };
        let tops = match self.rows {
            Rows::One => vec![STRIP_PADDING],
            Rows::Two => vec![STRIP_PADDING + height + ENTRY_GAP, STRIP_PADDING],
        };
        tops.into_iter().flat_map(row).take(letters).collect()
    }
}

fn strip_width(letters: f64, letter_width: Dp) -> Dp {
    strip_spacing(letters) + letter_width * letters
}

fn strip_spacing(letters: f64) -> Dp {
    at_both_ends(STRIP_PADDING) + ENTRY_GAP * (letters - 1.0)
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Form {
    Letters(Strip),
    List,
}

pub(crate) struct Placed {
    pub entries: Entries,
    pub frame: Rect,
    rects: Vec<Rect>,
    key: Rect,
    form: Form,
}

impl Placed {
    pub fn new(popup: &Popup, snapshot: &Snapshot, grid: Grid) -> Option<Self> {
        let entries = Entries::of(popup, snapshot)?;
        let key = grid.rect(&snapshot.board, popup.at);
        let (form, first, inside, padding) = match &entries {
            Entries::FurtherLetters(letters) => {
                let strip = Strip::new(letters, grid.panel_width());
                let first = Rect::new(STRIP_PADDING, STRIP_PADDING, strip.letter_width, key.height);
                let inside = strip.letters(letters.len(), key.height);
                (Form::Letters(strip), first, inside, STRIP_PADDING)
            }
            Entries::Layouts(layouts) => {
                let first = Rect::new(
                    LIST_PADDING,
                    LIST_PADDING + HEADING_HEIGHT + ENTRY_GAP,
                    LIST_WIDTH - at_both_ends(LIST_PADDING),
                    key.height,
                );
                let step = Point::new(Dp::ZERO, key.height + ENTRY_GAP);
                let inside: Vec<Rect> = successors(Some(first), |rect| Some(rect.translated(step)))
                    .take(layouts.len())
                    .collect();
                (Form::List, first, inside, LIST_PADDING)
            }
        };
        let content = (inside.iter()).fold(first, |content, rect| content.union(*rect));
        let size = Size::new(
            content.x + content.width + padding,
            content.y + content.height + padding,
        );
        let x = grid.fit_left(key.x, size.width);
        let y = grid.fit_top(key.y - ABOVE - size.height);
        let frame = Rect::new(x, y, size.width, size.height);
        let origin = Point::new(frame.x, frame.y);
        Some(Self {
            entries,
            frame,
            rects: inside
                .into_iter()
                .map(|rect| rect.translated(origin))
                .collect(),
            key,
            form,
        })
    }

    pub fn entry(&self, index: usize) -> Option<Rect> {
        self.rects.get(index).copied()
    }

    pub fn entry_at(&self, point: Point) -> Option<usize> {
        let frame = self.frame;
        match self.form {
            Form::Letters(strip) => {
                let inside = (frame.x..=frame.x + frame.width).contains(&point.x)
                    && (frame.y..=self.key.y + self.key.height).contains(&point.y);
                let half_gap = ENTRY_GAP / 2.0;
                let lower = || self.rects.iter().take(strip.columns);
                let column = lower()
                    .take_while(|rect| rect.x - half_gap <= point.x)
                    .count()
                    .saturating_sub(1);
                let upper = strip.rows == Rows::Two
                    && lower()
                        .next()
                        .is_some_and(|rect| point.y < rect.y - half_gap);
                let index = if upper {
                    strip.columns + column
                } else {
                    column
                };
                inside.then(|| index.min(self.rects.len().saturating_sub(1)))
            }
            Form::List => self.rects.iter().position(|rect| rect.contains(point)),
        }
    }
}
