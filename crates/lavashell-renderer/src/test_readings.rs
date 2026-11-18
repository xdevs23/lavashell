// SPDX-License-Identifier: GPL-3.0-only
use crate::{DrawFailure, Painter};

#[derive(Clone, Copy, Debug)]
pub struct TileUploads {
    pub images: usize,
    pub bytes: usize,
}

impl Painter {
    #[must_use]
    pub fn recorded_failures(&self, failure: DrawFailure) -> u64 {
        self.failures.count(failure)
    }

    #[must_use]
    pub fn retained_tile_count(&self) -> usize {
        self.resources.tiles.borrow().images().count()
    }

    #[must_use]
    pub fn pending_tile_uploads(&self) -> TileUploads {
        self.resources.tiles.borrow().pending_uploads()
    }
}
