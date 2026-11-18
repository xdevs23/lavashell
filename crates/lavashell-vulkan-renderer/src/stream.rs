// SPDX-License-Identifier: GPL-3.0-only
use crate::vulkan::VulkanRenderer;
use lavashell_core::prelude::Presented;
use lavashell_renderer::{FrameDevice, Painter, StreamBackend, StreamFrames};
use std::convert::Infallible;

pub trait RendererStreams: StreamBackend<Gpu = VulkanRenderer, Error: Send> + 'static {}

impl<S: StreamBackend<Gpu = VulkanRenderer, Error: Send> + 'static> RendererStreams for S {}

#[derive(Default)]
pub struct NoStreams {
    frames: StreamFrames,
}

#[cfg(test)]
impl NoStreams {
    pub(crate) fn frames_mut(&mut self) -> &mut StreamFrames {
        &mut self.frames
    }
}

impl StreamBackend for NoStreams {
    type Gpu = VulkanRenderer;
    type Error = Infallible;

    fn draw<T>(
        &mut self,
        gpu: &mut Self::Gpu,
        painter: &mut Painter,
        _: Presented<'_>,
        draw: impl FnOnce(FrameDevice<'_, Self::Gpu>) -> T,
    ) -> (T, Result<(), Infallible>) {
        let device = FrameDevice {
            gpu,
            painter,
            streams: &self.frames,
        };
        (draw(device), Ok(()))
    }
}
