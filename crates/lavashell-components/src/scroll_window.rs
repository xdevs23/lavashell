// SPDX-License-Identifier: GPL-3.0-only
//! The code shared by `Recycler` and `MeasuredRecycler`: the count of rows that the scroll passed since the last
//! composition, and the semantics of a list and of its items.

use lavashell_core::prelude::*;
use std::{cell::Cell, rc::Rc};

/// The scroll offset at the last composition of the rows of a list. The next composition uses the offset to also
/// compose the rows that the scroll passed since then.
#[derive(Clone, Default)]
pub(crate) struct ScrollWindow {
    composed_offset: Rc<Cell<Dp>>,
}

impl ScrollWindow {
    /// The number of rows to compose on each side of the rows in view after the scroll moved to
    /// `offset`: the number of rows that the scroll passed since the last call, at most `limit`.
    /// Callers pass the number of rows in one viewport as `limit`. A longer move is a jump, which
    /// composes `limit` rows on each side. `rows_since` returns the number of rows between a
    /// scroll offset and `offset`.
    pub(crate) fn overscan(
        &self,
        offset: Dp,
        limit: usize,
        rows_since: impl FnOnce(Dp) -> usize,
    ) -> usize {
        rows_since(self.composed_offset.replace(offset)).min(limit)
    }
}

/// The semantics of a list of `count` items in `columns` columns. When assistive technology asks to bring an item
/// into view, the list calls `reveal` with the index of that item and returns the error of `reveal`.
pub(crate) fn list(
    count: usize,
    columns: usize,
    reveal: impl Fn(usize) -> Result<(), ActionError> + 'static,
) -> Modifier {
    Modifier::new()
        .semantics(Semantics {
            role: Some(Role::List),
            collection: Some(CollectionInfo {
                rows: Some(count.div_ceil(columns)),
                columns: Some(columns),
                item_count: Some(count),
                ..CollectionInfo::default()
            }),
            ..Semantics::default()
        })
        .on_accessibility_action(ActionKind::RevealCollectionItem, move |action| {
            if let AccessibilityAction::RevealCollectionItem { row, column } = action
                && let Some(index) = row.checked_mul(columns).and_then(|n| n.checked_add(column))
                && column < columns
                && index < count
            {
                reveal(index)
            } else {
                Ok(())
            }
        })
}

/// The semantics of the list item at `row` and `column`.
pub(crate) fn item(row: usize, column: usize) -> Modifier {
    Modifier::new().semantics(Semantics {
        role: Some(Role::ListItem),
        mode: SemanticsMode::MergeDescendants,
        item: Some(CollectionItem {
            row,
            column,
            row_span: 1,
            column_span: 1,
            ..CollectionItem::default()
        }),
        ..Semantics::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_overscan_is_the_rows_since_the_last_composition_and_at_most_the_limit() {
        let window = ScrollWindow::default();
        let rows = |passed: usize, previous: Dp| {
            move |offset: Dp| {
                assert_eq!(offset, previous);
                passed
            }
        };
        assert_eq!(window.overscan(10.dp(), 3, rows(2, Dp::ZERO)), 2);
        assert_eq!(window.overscan(90.dp(), 3, rows(8, 10.dp())), 3);
    }
}
