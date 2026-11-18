// SPDX-License-Identifier: GPL-3.0-only
use crate::buffer_importer::HardwareBufferImporter;
use crate::error::Error;
use crate::platform::VulkanRenderer;
use crate::surface_stream::SurfaceStream;
use lavashell_core::prelude::{Presented, StreamId};
use lavashell_renderer::{FrameDevice, Latch, LatchedFrames, Painter, Stream, StreamBackend};
use std::collections::BTreeMap;

pub struct AospStreams {
    frames: LatchedFrames<HardwareBufferImporter>,
    attached: BTreeMap<StreamId, Stream<SurfaceStream>>,
}

impl AospStreams {
    pub(crate) fn new(importer: HardwareBufferImporter) -> Self {
        Self {
            frames: LatchedFrames::new(importer),
            attached: BTreeMap::new(),
        }
    }

    pub fn attach(&mut self, stream: SurfaceStream) -> StreamId {
        let stream = Stream::new(stream);
        let id = stream.id();
        let _fresh_id_replaces_nothing = self.attached.insert(id, stream);
        id
    }

    #[cfg(test)]
    pub fn detach(&mut self, id: StreamId) -> Option<Stream<SurfaceStream>> {
        self.attached.remove(&id)
    }

    pub fn stream(&mut self, id: StreamId) -> Result<&mut Stream<SurfaceStream>, Error> {
        find(&mut self.attached, id)
    }

    pub fn latch(&mut self, painter: &Painter, id: StreamId) -> Result<Latch, Error> {
        self.frames.latch(painter, find(&mut self.attached, id)?)
    }

    #[cfg(target_os = "android")]
    #[must_use]
    pub fn foreign_importer(&self) -> &lavashell_vulkan_renderer::ForeignImporter {
        &self.frames.importer().0
    }
}

impl StreamBackend for AospStreams {
    type Gpu = VulkanRenderer;
    type Error = Error;

    fn draw<T>(
        &mut self,
        gpu: &mut VulkanRenderer,
        painter: &mut Painter,
        tree: Presented<'_>,
        draw: impl FnOnce(FrameDevice<'_, VulkanRenderer>) -> T,
    ) -> (T, Result<(), Error>) {
        self.frames.draw(gpu, painter, tree, draw)
    }
}

fn find(
    attached: &mut BTreeMap<StreamId, Stream<SurfaceStream>>,
    id: StreamId,
) -> Result<&mut Stream<SurfaceStream>, Error> {
    attached.get_mut(&id).ok_or(Error::FindStream(id))
}
