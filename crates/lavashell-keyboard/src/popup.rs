// SPDX-License-Identifier: GPL-3.0-only
use crate::board::{Grid, KeyAt, at_both_ends};
use crate::keyboard::Snapshot;
use crate::layouts::Layout;
use lavashell_core::prelude::{Dp, Point, Rect, Size};
use std::iter::successors;

// The sizes of the two popups shown above a key: the strip of accented letters and the list of
// layouts.
pub(crate) const ENTRY_GAP: Dp = Dp::new(4.0);
pub(crate) const ENTRY_WIDTH: Dp = Dp::new(80.0);
pub(crate) const STRIP_PADDING: Dp = Dp::new(6.0);
pub(crate) const LIST_WIDTH: Dp = Dp::new(340.0);
pub(crate) const LIST_PADDING: Dp = Dp::new(8.0);
pub(crate) const HEADING_HEIGHT: Dp = Dp::new(34.0);

/// The space between a popup and the key below it.
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
    /// Open while the finger of the press with the serial number `press` is down. Lifting that
    /// finger chooses the entry under it.
    Held { press: u64, offer: Offer },
    /// The list of layouts, open after the finger lifted without choosing a layout.
    LayoutsLeft,
}

impl Popup {
    /// Opens the popup for the press with the serial number `press` on the key at `at`. A strip
    /// of letters starts with its first letter chosen, so lifting the finger without moving types
    /// that letter. A list of layouts starts with no layout chosen.
    pub fn new(press: u64, at: KeyAt, offer: Offer) -> Self {
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

    /// Whether the finger that opened the popup is still down.
    pub fn under_finger(&self) -> bool {
        matches!(self.open, Open::Held { .. })
    }

    pub fn held_by(&self, press: u64) -> bool {
        matches!(self.open, Open::Held { press: held, .. } if held == press)
    }
}

/// The entries shown in a popup. Letters are in lower case.
pub(crate) enum Entries {
    FurtherLetters(FurtherLetters),
    Layouts(Vec<&'static Layout>),
}

impl Entries {
    /// The entries shown by `popup` for the keyboard in `snapshot`, or `None` when the plain letter
    /// that opened the popup has no accented letters in that state, such as on a page of symbols.
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

/// The letters offered by a long press, each once, and how many they are as an `f64`.
pub(crate) struct FurtherLetters {
    letters: Vec<char>,
    /// Distinct characters are far fewer than the whole numbers that an `f64` holds exactly.
    count: f64,
}

impl FurtherLetters {
    /// The letters of `listed`, each once, in the order in which they are first listed.
    pub fn new(listed: impl IntoIterator<Item = char>) -> Self {
        let mut further = Self {
            letters: Vec::new(),
            count: 0.0,
        };
        for letter in listed {
            if !further.letters.contains(&letter) {
                further.letters.push(letter);
                further.count += 1.0;
            }
        }
        further
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

    /// The number of columns that `letters` letters take in these rows.
    fn columns(self, letters: f64) -> f64 {
        match self {
            Self::One => letters,
            Self::Two => (letters / 2.0).ceil(),
        }
    }
}

/// The letters of a strip lie in one row, or in two rows when one row would be wider than the
/// panel. When two rows are still wider than the panel, the letters are narrower than
/// `ENTRY_WIDTH`. The first letters lie in the lower row, nearest the key.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Strip {
    columns: usize,
    rows: Rows,
    letter_width: Dp,
}

impl Strip {
    /// The strip of `letters` above a panel `panel` wide.
    fn new(letters: &FurtherLetters, panel: Dp) -> Self {
        let rows = if strip_width(letters.count, ENTRY_WIDTH) > panel {
            Rows::Two
        } else {
            Rows::One
        };
        let across = rows.columns(letters.count);
        let room = (panel - strip_spacing(across)).max(Dp::ZERO) / across;
        Self {
            columns: letters.len().div_ceil(rows.count()),
            rows,
            letter_width: ENTRY_WIDTH.min(room),
        }
    }

    /// The rectangles of the first `letters` letters, each `height` tall, inside the strip.
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

/// The width of a strip of `letters` letters in one row, each `letter_width` wide.
fn strip_width(letters: f64, letter_width: Dp) -> Dp {
    strip_spacing(letters) + letter_width * letters
}

/// The width of the padding and the gaps of a strip of `letters` letters in one row.
fn strip_spacing(letters: f64) -> Dp {
    at_both_ends(STRIP_PADDING) + ENTRY_GAP * (letters - 1.0)
}

/// How the entries of a popup lie: as the letters of a strip, or as the list of layouts.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Form {
    Letters(Strip),
    List,
}

/// A popup with its entries, placed on the screen above the key that opened it, in the coordinates
/// of a grid.
pub(crate) struct Placed {
    pub entries: Entries,
    pub frame: Rect,
    /// The rectangles of the entries, in the order of the entries.
    rects: Vec<Rect>,
    /// The rectangle of the key that opened the popup.
    key: Rect,
    form: Form,
}

impl Placed {
    /// The popup placed in `grid`, or `None` when it shows no entries.
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

    /// The rectangle of the entry at `index`.
    pub fn entry(&self, index: usize) -> Option<Rect> {
        self.rects.get(index).copied()
    }

    /// The index of the entry chosen by a finger at `point`. In a list of layouts, that is the
    /// layout under the finger. In a strip of letters, it is the letter in the column and the row
    /// of the finger while the finger is between the top of the popup and the bottom of the key
    /// that opened the popup. Below the strip, the entry is the letter in the lower row and in the
    /// column of the finger.
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
