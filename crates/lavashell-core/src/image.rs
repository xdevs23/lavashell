// SPDX-License-Identifier: GPL-3.0-only
//! Images for draw commands: an encoded image file, and a frame from a stream rendered by another process. An
//! image contains no GPU resource, so draw commands that use it work with every renderer.

mod fit;
mod stream;

pub use fit::{Anchor, ImageFit, ImageFrame, ImagePlacement};
use std::fmt;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
pub use stream::{StreamId, StreamImage};

#[derive(Clone)]
pub struct Image {
    id: u64,
    width: u32,
    height: u32,
    encoded: Arc<[u8]>,
    decoded: Option<Arc<dyn std::any::Any + Send + Sync>>,
}

impl Image {
    /// An image from the encoded file in `bytes`. `width` and `height` are the size of the decoded image in pixels.
    /// The renderer skips an image whose decoded size differs from them.
    pub fn encoded(bytes: impl Into<Arc<[u8]>>, width: u32, height: u32) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self {
            id: NEXT.fetch_add(1, Ordering::Relaxed),
            width,
            height,
            encoded: bytes.into(),
            decoded: None,
        }
    }

    /// Attaches `decoded`, image data decoded on the CPU that never changes. A renderer that knows the type `T`
    /// draws from that data. Other renderers decode the encoded bytes instead. Any renderer on any thread can draw
    /// the image, so `decoded` must not contain a GPU resource tied to a device, a queue or a thread.
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
    pub fn id(&self) -> u64 {
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
