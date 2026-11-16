// SPDX-License-Identifier: GPL-3.0-only
use crate::geometry::PixelSize;
use crate::unique_id::UniqueId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StreamId(UniqueId);

impl StreamId {
    #[must_use]
    pub fn next() -> Self {
        Self(UniqueId::next())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StreamImage {
    pub stream: StreamId,
    pub generation: u64,
    pub size: PixelSize,
}
