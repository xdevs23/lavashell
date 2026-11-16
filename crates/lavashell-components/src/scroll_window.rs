// SPDX-License-Identifier: GPL-3.0-only

use lavashell_core::prelude::*;
use std::{cell::Cell, rc::Rc};

#[derive(Clone, Default)]
pub(crate) struct ScrollWindow {
    composed_offset: Rc<Cell<Dp>>,
}

impl ScrollWindow {
    pub(crate) fn overscan(
        &self,
        offset: Dp,
        limit: usize,
        rows_since: impl FnOnce(Dp) -> Result<usize, NumberError>,
    ) -> Result<usize, NumberError> {
        Ok(rows_since(self.composed_offset.replace(offset))?.min(limit))
    }
}

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
    use lavashell_core::numbers::whole_count;

    #[test]
    fn the_overscan_is_the_rows_since_the_last_composition_and_at_most_the_limit() {
        let window = ScrollWindow::default();
        let rows = |passed: usize, previous: Dp| {
            move |offset: Dp| {
                assert_eq!(offset, previous);
                Ok(passed)
            }
        };
        assert_eq!(window.overscan(10.dp(), 3, rows(2, Dp::ZERO)), Ok(2));
        assert_eq!(window.overscan(90.dp(), 3, rows(8, 10.dp())), Ok(3));
    }

    #[test]
    fn rows_that_cannot_be_counted_fail_the_overscan() {
        let window = ScrollWindow::default();
        let uncounted = |_: Dp| whole_count(f64::NAN);
        assert_eq!(
            window.overscan(10.dp(), 3, uncounted),
            Err(NumberError::NotFinite)
        );
    }
}
