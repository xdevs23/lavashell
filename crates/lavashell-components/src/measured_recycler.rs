// SPDX-License-Identifier: GPL-3.0-only
use crate::holder_pool::HolderPool;
use crate::scroll_window::{self, ScrollWindow};
use crate::{DuplicateKey, RecycledItem, RecyclerStats};
use lavashell_core::prelude::*;
use lavashell_core::view::CompositionError;
use measured_rows::{MeasuredRows, RowKeys};
use scroll_anchor::ScrollAnchor;
use std::{cell::RefCell, hash::Hash, ops::RangeInclusive, rc::Rc};

mod height_index;
mod measured_rows;
mod scroll_anchor;

/// A recycled list of rows whose heights are measured after composition. A row not measured yet
/// counts as high as the mean height of the measured rows. Only the rows in view and the rows that
/// the scroll passed since the last composition are composed.
pub struct MeasuredRecycler<K> {
    pub scroll: ScrollState,
    pool: HolderPool<K>,
    rows: State<MeasuredRows<K>>,
    anchor: Rc<RefCell<ScrollAnchor>>,
    /// The first and the last row in view.
    in_view: Derived<RangeInclusive<usize>>,
    at_end: Derived<bool>,
    window: ScrollWindow,
}

impl<K> Clone for MeasuredRecycler<K> {
    fn clone(&self) -> Self {
        Self {
            scroll: self.scroll.clone(),
            pool: self.pool.clone(),
            rows: self.rows.clone(),
            anchor: self.anchor.clone(),
            in_view: self.in_view.clone(),
            at_end: self.at_end.clone(),
            window: self.window.clone(),
        }
    }
}

impl<K: 'static> Default for MeasuredRecycler<K> {
    fn default() -> Self {
        let rows = State::new(MeasuredRows::new());
        let scroll = ScrollState::new(Axis::Vertical);
        let heights = rows.clone();
        let in_view = scroll.select(move |view| {
            heights
                .with(|rows| rows.find(view.offset)..=rows.find(view.offset + view.viewport.height))
        });
        let heights = rows.clone();
        let at_end = scroll.select(move |view| {
            heights.with(|rows| rows.total() - view.viewport.height - view.offset < rows.pixel())
        });
        Self {
            scroll,
            pool: HolderPool::default(),
            rows,
            anchor: Rc::default(),
            in_view,
            at_end,
            window: ScrollWindow::default(),
        }
    }
}

impl<K: Clone + Eq + Hash + 'static> MeasuredRecycler<K> {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn stats(&self) -> RecyclerStats {
        self.pool.stats()
    }

    /// Sets the keys of the rows in display order. Call it when the data or its order changes. A
    /// key that was in the list before gets the height measured for it before.
    ///
    /// # Errors
    ///
    /// Fails without a change to the list when two keys are equal.
    pub fn set_keys(&self, keys: Vec<K>) -> Result<(), DuplicateKey> {
        if self.rows.peek_with(|rows| rows.has_keys(&keys)) {
            return Ok(());
        }
        let keys = RowKeys::new(keys)?;
        self.rows.update(|rows| rows.replace_keys(keys));
        self.anchor.borrow_mut().reset();
        Ok(())
    }

    pub fn reveal(&self, position: usize) {
        let span = self.rows.peek_with(|rows| {
            (position < rows.len()).then(|| (rows.prefix(position), rows.prefix(position + 1)))
        });
        if let Some((start, end)) = span {
            self.scroll.reveal(start, end);
        }
    }

    pub fn bottom(&self) {
        let total = self.rows.peek_with(MeasuredRows::total);
        self.scroll
            .jump_to((total - self.scroll.viewport().height).max(Dp::ZERO));
    }

    #[must_use]
    pub fn at_bottom(&self) -> bool {
        self.at_end.get()
    }

    /// Sets `pixel` as the device pixel size of the rows when it differs from the size set before.
    /// The update recomputes the rows in view and whether the scroll is at the end.
    fn use_pixel(&self, pixel: Dp) {
        if self.rows.peek_with(|rows| rows.pixel() != pixel) {
            self.rows.update(|rows| rows.set_pixel(pixel));
        }
    }

    fn measured(&self, key: &K, width: Dp, measured: Dp, follow: bool) {
        let Some((slot, height)) = self
            .rows
            .peek_with(|rows| rows.accepted(key, width, measured))
        else {
            return;
        };
        self.rows.update(|rows| {
            let mut anchor = self.anchor.borrow_mut();
            let top = anchor.top(rows);
            rows.measure(slot, height);
            if follow {
                let end = rows.total() - self.scroll.viewport().height;
                anchor.follow_end(end.max(Dp::ZERO));
            } else {
                // A measured row above the anchor row moves the anchor row. A change of the mean
                // height also moves the anchor row when rows above it are not measured.
                let shift = anchor.top(rows) - top;
                anchor.shift(shift, || self.scroll.offset());
            }
        });
    }

    /// Corrects the scroll offset so the row at the top of the viewport remains in place, and
    /// returns whether the width changed.
    fn reanchor(&self, width: Dp) -> bool {
        if self.rows.peek_with(|rows| rows.has_width(width)) {
            return false;
        }
        let offset = self.scroll.peek_offset();
        let anchored = self.rows.update(|rows| {
            let slot = rows.find(offset);
            let inside = offset - rows.prefix(slot);
            rows.change_width(width);
            let anchored = rows.prefix(slot) + inside;
            self.anchor.borrow_mut().reanchor(slot, anchored);
            anchored
        });
        self.scroll.jump_to(anchored);
        true
    }

    /// Sets the row at the top of the viewport as the anchor row when the scroll moved since the
    /// last composition.
    fn anchor_at_scroll(&self) {
        let offset = self.scroll.peek_offset();
        self.rows
            .peek_with(|rows| self.anchor.borrow_mut().observe(rows, offset));
    }

    fn settle_scroll(&self) {
        let content = self.scroll.content().height;
        let settled = self
            .rows
            .peek_with(|rows| self.anchor.borrow_mut().settle(rows, content));
        if let Some(offset) = settled {
            self.scroll.jump_to(offset);
        }
    }

    /// Composes the rows in view and the rows that the scroll passed, each under its key and at
    /// its position in the list, and calls `bind` for each of them. A `RevealCollectionItem` action
    /// for a row that is not composed scrolls to that row without composing the rows before it.
    /// The row is composed in the next frame.
    /// `modifier` applies after the settings of the list node.
    pub fn compose(
        &self,
        ui: &mut Ui,
        modifier: Modifier,
        mut bind: impl FnMut(&mut Ui, RecycledItem<K>),
    ) {
        let pixel = ui.viewport().pixel();
        self.use_pixel(pixel);
        let width = self.scroll.viewport().width.max(pixel);
        let width_changed = self.reanchor(width);
        self.anchor_at_scroll();
        let laid_out = self.rows.with(|rows| {
            let following = self.anchor.borrow().follows();
            let follow = following || (!width_changed && self.at_end.get());
            let (first, last) = self.in_view.get().into_inner();
            let offset = self.scroll.peek_offset();
            let overscan = self.window.overscan(offset, last - first + 1, |previous| {
                first.abs_diff(rows.find(previous))
            });
            let start = first.saturating_sub(overscan);
            let end = (last + 1 + overscan).min(rows.len());
            let requested = (start..end)
                .map(|slot| (slot, rows.key(slot).clone(), 0))
                .collect();
            let bindings = self.pool.bind(rows.len(), requested, |slot| {
                Size::new(width, rows.height(slot))
            })?;
            let tops: Vec<_> = (bindings.iter())
                .map(|item| rows.prefix(item.index))
                .collect();
            Ok::<_, CompositionError>((rows.total(), rows.len(), follow, bindings, tops))
        });
        let (extent, count, follow, bindings, tops) = match laid_out {
            Ok(laid_out) => laid_out,
            Err(error) => {
                ui.fail(error);
                return;
            }
        };
        let (revealing, settling) = (self.clone(), self.clone());
        let list = scroll_window::list(count, 1, move |row| {
            revealing.reveal(row);
            Ok(())
        })
        .on_measure(move |_| settling.settle_scroll());
        ui.scroll(self.scroll.clone(), list.then(modifier), |ui| {
            ui.layout(
                Arrangement::Overlay,
                Modifier::new()
                    .fill_max_width()
                    .min_size(Dp::ZERO, extent)
                    .semantics(Semantics {
                        mode: SemanticsMode::Decorative,
                        ..Semantics::default()
                    }),
                |ui| {
                    for (item, top) in bindings.into_iter().zip(tops) {
                        let recycler = self.clone();
                        let key = item.key.clone();
                        let row = item.index;
                        ui.key(
                            item.key.clone(),
                            scroll_window::item(row, 0)
                                .width(width)
                                .offset(Dp::ZERO, top)
                                .on_measure(move |size| {
                                    recycler.measured(&key, width, size.height, follow);
                                }),
                            |ui| bind(ui, item),
                        );
                    }
                },
            );
        });
    }
}

#[cfg(test)]
#[path = "tests/measured_recycling.rs"]
mod tests;
