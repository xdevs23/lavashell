// SPDX-License-Identifier: GPL-3.0-only

mod fit;
mod stream;

use crate::unique_id::UniqueId;
pub use fit::{Anchor, ImageFit, ImageFrame, ImagePlacement};
use std::fmt;
use std::sync::Arc;
pub use stream::{StreamId, StreamImage};

#[derive(Clone)]
pub struct Image {
    id: UniqueId,
    width: u32,
    height: u32,
    encoded: Arc<[u8]>,
    decoded: Option<Arc<dyn std::any::Any + Send + Sync>>,
}

impl Image {
    pub fn encoded(bytes: impl Into<Arc<[u8]>>, width: u32, height: u32) -> Self {
        Self {
            id: UniqueId::next(),
            width,
            height,
            encoded: bytes.into(),
            decoded: None,
        }
    }

    #[must_use]
    pub fn with_backend_data<T: std::any::Any + Send + Sync>(mut self, decoded: T) -> Self {
        self.decoded = Some(Arc::new(decoded));
        self
    }

    #[must_use]
    pub fn backend_data<T: std::any::Any + Send + Sync>(&self) -> Option<&T> {
        self.decoded.as_ref()?.downcast_ref()
    }

    #[must_use]
    pub fn id(&self) -> UniqueId {
        self.id
    }

    #[must_use]
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.encoded
    }

    #[must_use]
    pub fn data(&self) -> &Arc<[u8]> {
        &self.encoded
    }
}

impl fmt::Debug for Image {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Image")
            .field("id", &self.id)
            .field("width", &self.width)
            .field("height", &self.height)
            .finish_non_exhaustive()
    }
}

impl PartialEq for Image {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
