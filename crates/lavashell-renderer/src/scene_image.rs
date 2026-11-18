// SPDX-License-Identifier: GPL-3.0-only

use crate::Error;
use skia_safe::Canvas;
use skia_safe::gpu::{DirectContext, DirectContextId};

pub trait SceneImageLease {}

pub struct SceneImage {
    importer: DirectContextId,
    image: skia_safe::Image,
    lease: Option<Box<dyn SceneImageLease>>,
}

impl SceneImage {
    #[must_use]
    pub fn new(
        importer: &DirectContext,
        image: skia_safe::Image,
        lease: Option<Box<dyn SceneImageLease>>,
    ) -> Self {
        Self {
            importer: importer.id(),
            image,
            lease,
        }
    }

    pub(crate) fn layer_of<'a>(&'a self, canvas: &'a Canvas) -> Result<SceneLayer<'a>, Error> {
        let canvas_owner = canvas.direct_context().map(|context| context.id());
        if canvas_owner != Some(self.importer) {
            return Err(Error::SceneOfAnotherRenderer {
                importer: self.importer,
                canvas_owner,
            });
        }
        Ok(SceneLayer {
            image: &self.image,
            canvas,
        })
    }
}

impl Drop for SceneImage {
    fn drop(&mut self) {
        drop(self.lease.take());
    }
}

pub(crate) struct SceneLayer<'a> {
    image: &'a skia_safe::Image,
    canvas: &'a Canvas,
}

impl SceneLayer<'_> {
    pub(crate) fn draw(&self) {
        let layer = skia_safe::Rect::from_isize(self.canvas.base_layer_size());
        let paint = skia_safe::Paint::default();
        let saved = self.canvas.save();
        let _canvas_returned_for_chaining = self
            .canvas
            .reset_matrix()
            .draw_image_rect(self.image, None, layer, &paint)
            .restore_to_count(saved);
    }
}
