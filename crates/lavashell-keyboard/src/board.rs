// SPDX-License-Identifier: GPL-3.0-only
use crate::layouts::Layout;
use lavashell_core::prelude::{Dp, Insets, Key as KeyCode, Point, Rect};
use std::iter::repeat_n;

/// The space between two keys and between two rows.
pub(crate) const GAP: Dp = Dp::new(8.0);

/// Key shapes closer than this distance merge into one shape. Keys that enter enlarged touch their
/// neighbours, which are one gap away at rest. The bubble of a key preview sits on its key and
/// merges with it.
pub(crate) const FUSE_DISTANCE: Dp = Dp::new(GAP.get() / 2.0);

/// The space between the panel and its rows of keys.
pub(crate) const PADDING: Insets = Insets {
    top: Dp::new(12.0),
    right: Dp::new(12.0),
    bottom: Dp::new(10.0),
    left: Dp::new(12.0),
};

/// The number of rows of keys: three rows of characters and the bottom row.
pub(crate) const ROWS: f64 = 4.0;

/// The number of each row from the top, from `0.0` up to the last row.
pub(crate) fn row_numbers() -> impl Iterator<Item = f64> {
    (0_u32..).map(f64::from).take_while(|row| *row < ROWS)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Page {
    Letters,
    Symbols,
    MoreSymbols,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Key {
    Char(char),
    Currency,
    Shift,
    Delete,
    Page(Page),
    Globe,
    Hide,
    Space,
    Action,
}

impl Key {
    /// Whether the key acts when it is pressed, where the other keys act when they are released.
    pub fn acts_on_press(self) -> bool {
        matches!(self, Self::Delete)
    }
}

/// A key, or the empty space beside the keys of a short row, and its width in columns.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Slot {
    pub key: Option<Key>,
    pub span: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct KeyAt {
    pub row: usize,
    pub slot: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Board {
    pub columns: f64,
    pub rows: Vec<Vec<Slot>>,
}

/// The top and the third row of the first page of symbols.
const SYMBOLS: [&str; 2] = ["1234567890", "*\"':;!?"];

/// The middle row of the first page of symbols, which has the currency key between its parts.
const SYMBOLS_MIDDLE: [&str; 2] = ["@#", "_&-+()/"];

const MORE_SYMBOLS: [&str; 3] = ["~`|•√π÷×¶∆", "^°={}\\%", "©®™✓[]§"];

/// The currencies on the second page of symbols. The currency of the layout, which is on the first
/// page, is left out.
const CURRENCIES: [&str; 4] = ["€", "£", "$", "¥"];

/// The number of columns that a character key spans. With two columns for each key, a row with one
/// key fewer is centered with half a key of space on each side.
pub(crate) const CHARACTER: f64 = 2.0;

/// The number of columns that the hide key in the bottom row spans.
const HIDE: f64 = 2.0;

const PAGE: f64 = 3.0;
const ACTION: f64 = 3.0;

/// The number of columns spanned by the keys that share the bottom row with the space bar: the hide
/// key, the key that switches the page, a third key such as the globe key, the period and the
/// action key.
const BESIDE_SPACE_BAR: f64 = HIDE + PAGE + CHARACTER + CHARACTER + ACTION;

/// The smallest number of letters in the top row. The top row sets the number of columns. The
/// bottom row needs that many columns for the keys beside the space bar and at least one column for
/// the space bar.
pub(crate) const MIN_TOP_ROW: f64 = ((BESIDE_SPACE_BAR + 1.0) / CHARACTER).ceil();

impl Board {
    /// The keys of `page` in `layout`. The pages of symbols show the currency of `layout`. The page
    /// of letters has a globe key when `layouts` contains more than one layout.
    pub fn new(layout: &Layout, page: Page, layouts: &[&Layout]) -> Self {
        let globe = layouts.len() > 1;
        match page {
            Page::Letters => Self::page(
                layout.rows().map(characters),
                Key::Shift,
                Page::Symbols,
                globe.then_some(Key::Globe),
            ),
            Page::Symbols => {
                let [top, bottom] = SYMBOLS.map(characters);
                let [before, after] = SYMBOLS_MIDDLE.map(characters);
                let middle = (before.into_iter().chain([Key::Currency]).chain(after)).collect();
                let edge = Key::Page(Page::MoreSymbols);
                Self::page(
                    [top, middle, bottom],
                    edge,
                    Page::Letters,
                    Some(Key::Char(',')),
                )
            }
            Page::MoreSymbols => {
                let [top, rest, bottom] = MORE_SYMBOLS.map(characters);
                let others = (CURRENCIES.iter())
                    .filter(|other| **other != layout.currency())
                    .filter_map(|other| other.chars().next())
                    .take(CURRENCIES.len() - 1)
                    .map(Key::Char);
                let middle = others.chain(rest).collect();
                let edge = Key::Page(Page::Symbols);
                Self::page(
                    [top, middle, bottom],
                    edge,
                    Page::Letters,
                    Some(Key::Char(',')),
                )
            }
        }
    }

    /// The four rows of a page built from three rows of character keys. The top row sets the number
    /// of columns. The middle row is centered. The third row has `edge` on its left and the delete
    /// key on its right. The bottom row has the key that switches to `page`, `third` when given,
    /// and the space bar in the columns left over. `Layout::new` rejects lower rows of letters that
    /// are longer than the top row. The rows of symbols are written with the longest row on top.
    fn page(rows: [Vec<Key>; 3], edge: Key, page: Page, third: Option<Key>) -> Self {
        let [top, middle, bottom] = rows.map(|keys| {
            (keys.into_iter())
                .map(|key| slot(key, CHARACTER))
                .collect::<Vec<_>>()
        });
        let columns = spans(&top);
        let side = each_side(columns, &middle);
        let spacer = (side > 0.0).then_some(Slot {
            key: None,
            span: side,
        });
        let edges = each_side(columns, &bottom);
        let beside = [slot(Key::Hide, HIDE), slot(Key::Page(page), PAGE)]
            .into_iter()
            .chain(third.map(|key| slot(key, CHARACTER)));
        let after = [slot(Key::Char('.'), CHARACTER), slot(Key::Action, ACTION)];
        let fixed: f64 = (beside.clone().chain(after)).map(|slot| slot.span).sum();
        let space = columns - fixed;
        let rows = vec![
            top,
            (spacer.into_iter()).chain(middle).chain(spacer).collect(),
            std::iter::once(slot(edge, edges))
                .chain(bottom)
                .chain([slot(Key::Delete, edges)])
                .collect(),
            (beside.chain([slot(Key::Space, space)]).chain(after)).collect(),
        ];
        Self { columns, rows }
    }

    pub fn key(&self, at: KeyAt) -> Option<Key> {
        self.rows.get(at.row)?.get(at.slot)?.key
    }

    pub fn keys(&self) -> impl Iterator<Item = (KeyAt, Key)> + '_ {
        (self.rows.iter().enumerate()).flat_map(|(row, slots)| {
            (slots.iter().enumerate())
                .filter_map(move |(slot, entry)| Some((KeyAt { row, slot }, entry.key?)))
        })
    }

    fn start(&self, at: KeyAt) -> f64 {
        spans(&self.rows[at.row][..at.slot])
    }

    /// The middle of the slot at `at`, in half columns.
    fn middle(&self, at: KeyAt) -> f64 {
        2.0 * self.start(at) + self.rows[at.row][at.slot].span
    }

    fn nearest_in(&self, row: usize, middle: f64) -> Option<KeyAt> {
        let distance = |at: KeyAt| (self.middle(at) - middle).abs();
        (self.keys())
            .filter(|(at, _)| at.row == row)
            .min_by(|(first, _), (second, _)| distance(*first).total_cmp(&distance(*second)))
            .map(|(at, _)| at)
    }

    /// The key at `at`, or the key nearest to it when `at` is a position on another page.
    pub fn nearest(&self, at: KeyAt) -> Option<KeyAt> {
        let row = at.row.min(self.rows.len().checked_sub(1)?);
        let slot = at.slot.min(self.rows[row].len().checked_sub(1)?);
        self.nearest_in(row, self.middle(KeyAt { row, slot }))
    }

    /// The key that an arrow key moves to from `from`: the previous or next key in reading order,
    /// or the nearest key in the row above or below.
    pub fn step(&self, from: KeyAt, arrow: KeyCode) -> Option<KeyAt> {
        let keys: Vec<_> = self.keys().map(|(at, _)| at).collect();
        let index = keys.iter().position(|at| *at == from)?;
        match arrow {
            KeyCode::Left => index.checked_sub(1).map(|index| keys[index]),
            KeyCode::Right => keys.get(index + 1).copied(),
            KeyCode::Up => self.nearest_in(from.row.checked_sub(1)?, self.middle(from)),
            KeyCode::Down => self.nearest_in(from.row + 1, self.middle(from)),
            _ => None,
        }
    }
}

/// The columns that `slots` span together.
fn spans(slots: &[Slot]) -> f64 {
    slots.iter().map(|slot| slot.span).sum()
}

/// The whole columns on each side of `slots` centered in a row of `columns`.
fn each_side(columns: f64, slots: &[Slot]) -> f64 {
    ((columns - spans(slots)) / 2.0).floor()
}

/// The length added to an extent by `length` at each of its two ends.
pub(crate) fn at_both_ends(length: Dp) -> Dp {
    length * 2.0
}

fn characters(row: &str) -> Vec<Key> {
    row.chars().map(Key::Char).collect()
}

fn slot(key: Key, span: f64) -> Slot {
    Slot {
        key: Some(key),
        span,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Grid {
    column: Dp,
    key_height: Dp,
    panel_width: Dp,
}

impl Grid {
    pub fn new(panel_width: Dp, columns: f64, key_height: Dp) -> Self {
        let inner = panel_width - PADDING.left - PADDING.right;
        let gaps = GAP * (columns - 1.0);
        Self {
            column: (inner - gaps) / columns,
            key_height,
            panel_width,
        }
    }

    /// The left edge nearest to `x` of a box `width` wide that fits within the panel.
    pub fn fit_left(&self, x: Dp, width: Dp) -> Dp {
        x.min(self.panel_width - width).max(Dp::ZERO)
    }

    /// The height of the panel of keys that are `key_height` tall.
    pub fn panel_height(key_height: Dp) -> Dp {
        key_height * ROWS + GAP * (ROWS - 1.0) + PADDING.top + PADDING.bottom
    }

    /// The height of a key in a panel `panel_height` tall, or zero for a panel too short for its
    /// padding and gaps.
    pub fn key_height_in(panel_height: Dp) -> Dp {
        let rows = panel_height - GAP * (ROWS - 1.0) - PADDING.top - PADDING.bottom;
        rows.max(Dp::ZERO) / ROWS
    }

    pub fn key_height(&self) -> Dp {
        self.key_height
    }

    pub fn width(&self, span: f64) -> Dp {
        self.column * span + GAP * (span - 1.0)
    }

    pub fn rect(&self, board: &Board, at: KeyAt) -> Rect {
        let origin = Point::new(PADDING.left, PADDING.top);
        let above: Dp = repeat_n(self.key_height + GAP, at.row).sum();
        Rect::new(
            origin.x + (self.column + GAP) * board.start(at),
            origin.y + above,
            self.width(board.rows[at.row][at.slot].span),
            self.key_height,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layouts::{LAYOUTS, Layout, LayoutError, LayoutSpec};
    use icu_locale_core::subtags::language;
    use lavashell_core::numbers::count_f64;

    fn abc(rows: [&'static str; 3]) -> LayoutSpec {
        LayoutSpec {
            id: "xx-abc",
            language: language!("xx"),
            name: "X",
            arrangement: "ABC",
            rows,
            further_letters: &[],
            currency: "$",
        }
    }

    #[test]
    fn every_row_of_every_page_fills_the_columns_of_its_top_row() {
        for layout in LAYOUTS {
            for configured in [&LAYOUTS[..1], &LAYOUTS[..]] {
                for page in [Page::Letters, Page::Symbols, Page::MoreSymbols] {
                    let board = Board::new(layout, page, configured);
                    for row in &board.rows {
                        let gap = (spans(row) - board.columns).abs();
                        assert!(
                            gap <= board.columns * f64::EPSILON,
                            "{} {page:?}",
                            layout.id()
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn every_page_has_the_rows_of_the_panel() {
        for layout in LAYOUTS {
            for page in [Page::Letters, Page::Symbols, Page::MoreSymbols] {
                let board = Board::new(layout, page, &[]);
                assert_eq!(
                    count_f64(board.rows.len()),
                    Ok(ROWS),
                    "{} {page:?}",
                    layout.id()
                );
            }
        }
    }

    #[test]
    fn the_row_numbers_count_the_rows_of_the_panel() {
        assert_eq!(count_f64(row_numbers().count()), Ok(ROWS));
    }

    #[test]
    fn a_top_row_of_the_fewest_letters_leaves_the_space_bar_a_column() -> Result<(), LayoutError> {
        let narrow = Layout::new(abc(["abcdefg", "hijklm", "nop"]))?;
        assert_eq!(count_f64(narrow.rows()[0].chars().count()), Ok(MIN_TOP_ROW));
        for page in [Page::Letters, Page::Symbols, Page::MoreSymbols] {
            let board = Board::new(&narrow, page, &[&narrow, &narrow]);
            let space = (board.rows.iter().flatten())
                .find(|slot| slot.key == Some(Key::Space))
                .map(|slot| slot.span);
            assert!(space >= Some(1.0), "{page:?}");
        }
        let too_short = Layout::new(abc(["abcdef", "ghijk", "lmn"]));
        assert!(too_short.is_err());
        Ok(())
    }

    #[test]
    fn the_height_of_a_key_is_what_the_panel_leaves_after_its_padding_and_gaps() {
        let key_height = Dp::new(76.0);
        let panel = Grid::panel_height(key_height);
        assert_eq!(Grid::key_height_in(panel), key_height);
        assert_eq!(Grid::key_height_in(Dp::ZERO), Dp::ZERO);
    }
}
