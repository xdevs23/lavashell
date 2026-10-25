// SPDX-License-Identifier: GPL-3.0-only
use crate::geometry::PixelSize;
use std::sync::atomic::{AtomicU64, Ordering};

/// The number of a stream of frames rendered by another process. Each number is unique within this process.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StreamId(u64);

impl StreamId {
    pub fn next() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

/// A frame taken from a stream. The renderer supplies the pixels of the frame when it paints the frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StreamImage {
    pub stream: StreamId,
    /// Differs for every frame taken from the stream.
    pub generation: u64,
    pub size: PixelSize,
}
