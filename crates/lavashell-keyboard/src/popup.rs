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

    pub fn held_by(&self, press: u64) -> bool {
        matches!(self.open, Open::Held { press: held, .. } if held == press)
    }
}

/// The entries shown in a popup. Letters are in lower case.
pub(crate) enum Entries {
    FurtherLetters(Vec<char>),
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

pub(crate) struct Placed {
    pub entries: Entries,
    pub frame: Rect,
    /// The rectangles of the entries in the panel, in the order of the entries.
    rects: Vec<Rect>,
    /// The rectangle of the key that opened the popup.
    key: Rect,
}

impl Placed {
    pub fn new(popup: &Popup, snapshot: &Snapshot, grid: Grid) -> Option<Self> {
        let entries = Entries::of(popup, snapshot)?;
        let key = grid.rect(&snapshot.board, popup.at);
        let (first, step, padding) = match entries {
            Entries::FurtherLetters(_) => (
                Rect::new(STRIP_PADDING, STRIP_PADDING, ENTRY_WIDTH, key.height),
                Point::new(ENTRY_WIDTH + ENTRY_GAP, Dp::ZERO),
                STRIP_PADDING,
            ),
            Entries::Layouts(_) => (
                Rect::new(
                    LIST_PADDING,
                    LIST_PADDING + HEADING_HEIGHT + ENTRY_GAP,
                    LIST_WIDTH - at_both_ends(LIST_PADDING),
                    key.height,
                ),
                Point::new(Dp::ZERO, key.height + ENTRY_GAP),
                LIST_PADDING,
            ),
        };
        let inside: Vec<Rect> = successors(Some(first), |rect| Some(rect.translated(step)))
            .take(entries.len())
            .collect();
        let content = (inside.iter()).fold(first, |content, rect| content.union(*rect));
        let size = Size::new(
            content.x + content.width + padding,
            content.y + content.height + padding,
        );
        let x = grid.fit_left(key.x, size.width);
        let frame = Rect::new(x, key.y - ABOVE - size.height, size.width, size.height);
        let origin = Point::new(frame.x, frame.y);
        Some(Self {
            entries,
            frame,
            rects: inside
                .into_iter()
                .map(|rect| rect.translated(origin))
                .collect(),
            key,
        })
    }

    /// The rectangle of the entry at `index` in the panel.
    pub fn entry(&self, index: usize) -> Option<Rect> {
        self.rects.get(index).copied()
    }

    /// The index of the entry chosen by a finger at `point` in the panel. In a list of layouts,
    /// that is the layout under the finger. In a strip of letters, it is the letter in the column
    /// of the finger while the finger is between the top of the popup and the bottom of the key
    /// that opened the popup.
    pub fn entry_at(&self, point: Point) -> Option<usize> {
        let frame = self.frame;
        match self.entries {
            Entries::FurtherLetters(_) => {
                let inside = (frame.x..=frame.x + frame.width).contains(&point.x)
                    && (frame.y..=self.key.y + self.key.height).contains(&point.y);
                let half_gap = ENTRY_GAP / 2.0;
                let started = (self.rects.iter())
                    .take_while(|rect| rect.x - half_gap <= point.x)
                    .count();
                inside.then(|| started.saturating_sub(1))
            }
            Entries::Layouts(_) => self.rects.iter().position(|rect| rect.contains(point)),
        }
    }
}
