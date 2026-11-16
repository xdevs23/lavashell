// SPDX-License-Identifier: GPL-3.0-only
use crate::holder_pool::HolderPool;
use crate::scroll_window::{self, ScrollWindow};
use crate::{RecycledItem, RecyclerStats, RevealError, ViewTypeId};
use lavashell_core::numbers::{count_f64, whole_count};
use lavashell_core::prelude::*;
use lavashell_core::view::CompositionError;
use std::{hash::Hash, ops::Range};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RecyclerLayout {
    List {
        item_height: PositiveDp,
    },
    Grid {
        item_height: PositiveDp,
        min_item_width: PositiveDp,
        gap: NonNegativeDp,
    },
}

impl RecyclerLayout {
    #[must_use]
    pub fn list(item_height: PositiveDp) -> Self {
        Self::List { item_height }
    }

    #[must_use]
    pub fn grid(item_height: PositiveDp, min_item_width: PositiveDp, gap: NonNegativeDp) -> Self {
        Self::Grid {
            item_height,
            min_item_width,
            gap,
        }
    }
}

pub struct RecyclerItems<Key, ViewType> {
    pub count: usize,
    pub key: Key,
    pub view_type: ViewType,
}

type Placed<K> = Vec<(RecycledItem<K>, Point)>;

#[derive(Clone, Copy, PartialEq)]
struct LaidOut {
    count: usize,
    layout: RecyclerLayout,
    pixel: Dp,
}

pub struct Recycler<K> {
    pub scroll: ScrollState,
    pool: HolderPool<K>,
    laid_out: State<Option<LaidOut>>,
    in_view: Derived<Result<Range<usize>, NumberError>>,
    window: ScrollWindow,
}

impl<K> Clone for Recycler<K> {
    fn clone(&self) -> Self {
        Self {
            scroll: self.scroll.clone(),
            pool: self.pool.clone(),
            laid_out: self.laid_out.clone(),
            in_view: self.in_view.clone(),
            window: self.window.clone(),
        }
    }
}

impl<K> Default for Recycler<K> {
    fn default() -> Self {
        let scroll = ScrollState::new(Axis::Vertical);
        let laid_out = State::new(None);
        let source = laid_out.clone();
        let in_view = scroll.select(move |view| {
            source.get().map_or(Ok(0..0), |last: LaidOut| {
                ItemGrid::new(view.viewport, last.count, last.layout, last.pixel)?
                    .in_view(view.offset)
            })
        });
        Self {
            scroll,
            pool: HolderPool::default(),
            laid_out,
            in_view,
            window: ScrollWindow::default(),
        }
    }
}

impl<K> Recycler<K> {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn stats(&self) -> RecyclerStats {
        self.pool.stats()
    }

    pub fn reveal(&self, index: usize) -> Result<(), RevealError> {
        let Some(last) = self.laid_out.peek() else {
            return Ok(());
        };
        let grid = ItemGrid::new(self.scroll.viewport(), last.count, last.layout, last.pixel)
            .map_err(RevealError)?;
        let top = grid.top(index / grid.columns).map_err(RevealError)?;
        self.scroll.reveal(top, top + grid.height);
        Ok(())
    }
}

struct ItemGrid {
    count: usize,
    columns: usize,
    pitch: Dp,
    height: Dp,
    width: Dp,
    gap: Dp,
    extent: Dp,
    visible: Dp,
}

impl ItemGrid {
    fn new(
        viewport: Size,
        count: usize,
        layout: RecyclerLayout,
        pixel: Dp,
    ) -> Result<Self, NumberError> {
        let (height, fit, gap) = match layout {
            RecyclerLayout::List { item_height } => (item_height.get().max(pixel), 1.0, Dp::ZERO),
            RecyclerLayout::Grid {
                item_height,
                min_item_width,
                gap,
            } => {
                let gap = gap.get();
                let item_width = min_item_width.get().max(pixel);
                let fit = ((viewport.width + gap) / (item_width + gap)).floor();
                (item_height.get().max(pixel), fit.max(1.0), gap)
            }
        };
        let columns = whole_count(fit)?;
        let pitch = height + gap;
        let extent = (pitch * count_f64(count.div_ceil(columns))? - gap).max(Dp::ZERO);
        Ok(Self {
            count,
            columns,
            pitch,
            height,
            gap,
            width: ((viewport.width - gap * (fit - 1.0)) / fit).max(Dp::ZERO),
            extent,
            visible: viewport.height,
        })
    }

    fn top(&self, row: usize) -> Result<Dp, NumberError> {
        Ok(self.pitch * count_f64(row)?)
    }

    fn offset(&self, index: usize) -> Result<Point, NumberError> {
        let left = (self.width + self.gap) * count_f64(index % self.columns)?;
        Ok(Point::new(left, self.top(index / self.columns)?))
    }

    fn in_view(&self, offset: Dp) -> Result<Range<usize>, NumberError> {
        let offset = offset.clamp(Dp::ZERO, (self.extent - self.visible).max(Dp::ZERO));
        let first = whole_count((offset / self.pitch).floor())?;
        let last = whole_count(((offset + self.visible) / self.pitch).ceil())?;
        Ok(first..last.max(first + 1).min(self.count.div_ceil(self.columns)))
    }

    fn rows_in(&self, length: Dp) -> Result<usize, NumberError> {
        whole_count((length / self.pitch).ceil())
    }

    fn items(&self, rows: &Range<usize>, overscan: usize) -> Range<usize> {
        let first_row = rows.start.saturating_sub(overscan);
        let end_row = rows.end.saturating_add(overscan);
        let item = |row: usize| row.saturating_mul(self.columns).min(self.count);
        item(first_row)..item(end_row)
    }
}

impl<K: Clone + Eq + Hash + 'static> Recycler<K> {
    pub fn compose(
        &self,
        ui: &mut Ui,
        items: RecyclerItems<impl Fn(usize) -> K, impl Fn(usize) -> ViewTypeId>,
        layout: RecyclerLayout,
        modifier: Modifier,
        bind: impl FnMut(&mut Ui, RecycledItem<K>),
    ) {
        let pixel = ui.viewport().pixel();
        let count = items.count;
        match self.place(items, layout, pixel) {
            Ok((grid, placed)) => self.compose_placed(ui, count, &grid, placed, modifier, bind),
            Err(error) => ui.fail(error),
        }
    }

    fn place(
        &self,
        items: RecyclerItems<impl Fn(usize) -> K, impl Fn(usize) -> ViewTypeId>,
        layout: RecyclerLayout,
        pixel: Dp,
    ) -> Result<(ItemGrid, Placed<K>), CompositionError> {
        let RecyclerItems {
            count,
            key,
            view_type,
        } = items;
        let grid = ItemGrid::new(self.scroll.viewport(), count, layout, pixel)?;
        self.laid_out.set_if_changed(Some(LaidOut {
            count,
            layout,
            pixel,
        }));
        let offset = self.scroll.peek_offset();
        let overscan = self
            .window
            .overscan(offset, grid.rows_in(grid.visible)?, |previous| {
                grid.rows_in((offset - previous).abs())
            })?;
        let items = grid.items(&self.in_view.get()?, overscan);
        let requested: Vec<_> = items
            .map(|index| (index, key(index), view_type(index)))
            .collect();
        let bindings = self
            .pool
            .bind(count, requested, |_| Size::new(grid.width, grid.height))?;
        let placed = (bindings.into_iter())
            .map(|item| grid.offset(item.index).map(|offset| (item, offset)))
            .collect::<Result<_, _>>()?;
        Ok((grid, placed))
    }

    fn compose_placed(
        &self,
        ui: &mut Ui,
        count: usize,
        grid: &ItemGrid,
        placed: Placed<K>,
        modifier: Modifier,
        mut bind: impl FnMut(&mut Ui, RecycledItem<K>),
    ) {
        let reveal = self.clone();
        let columns = grid.columns;
        let revealing = move |index| {
            reveal
                .reveal(index)
                .map_err(|RevealError(cause)| ActionError::Count(cause))
        };
        ui.scroll(
            self.scroll.clone(),
            scroll_window::list(count, columns, revealing).then(modifier),
            |ui| {
                ui.layout(
                    Arrangement::Overlay,
                    Modifier::new()
                        .fill_max_width()
                        .height(grid.extent)
                        .semantics(Semantics {
                            mode: SemanticsMode::Decorative,
                            ..Semantics::default()
                        }),
                    |ui| {
                        for (item, offset) in placed {
                            let (row, column) = (item.index / columns, item.index % columns);
                            let identity = item.key.clone();
                            ui.key(
                                identity,
                                scroll_window::item(row, column)
                                    .size(grid.width, grid.height)
                                    .offset(offset.x, offset.y),
                                |ui| bind(ui, item),
                            );
                        }
                    },
                );
            },
        );
    }
}
