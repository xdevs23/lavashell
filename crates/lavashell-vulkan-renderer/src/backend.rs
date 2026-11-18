// SPDX-License-Identifier: GPL-3.0-only
use crate::vulkan::{HeadlessPresentation, PresentationBackend, Window};
use crate::{NoStreams, RendererSetup, RendererState, RendererStreams, StartError};
use lavashell_platform::{Backend, SurfaceControl};
use lavashell_renderer::StreamBackend;
use std::sync::Arc;

type StreamSetup<S> =
    Box<dyn FnOnce(&<S as StreamBackend>::Gpu) -> Result<S, <S as StreamBackend>::Error> + Send>;

type StartResult<C, S> = Result<RendererState<C, S>, StartError<<S as StreamBackend>::Error>>;

pub type VulkanWindow = Arc<dyn Window>;

pub struct VulkanBackend<P, S: StreamBackend = NoStreams> {
    pub presentation: P,
    pub streams: StreamSetup<S>,
}

impl<P> VulkanBackend<P> {
    pub fn without_streams(presentation: P) -> Self {
        Self {
            presentation,
            streams: Box::new(|_| Ok(NoStreams::default())),
        }
    }
}

pub type HeadlessBackend = VulkanBackend<HeadlessPresentation>;

impl HeadlessBackend {
    #[must_use]
    pub fn with_surfaces() -> Self {
        Self::without_streams(HeadlessPresentation::with_surfaces())
    }

    #[must_use]
    pub fn without_surfaces() -> Self {
        Self::without_streams(HeadlessPresentation::without_surfaces())
    }
}

impl<P: PresentationBackend + Send + 'static, S: StreamBackend + 'static> Backend
    for VulkanBackend<P, S>
{
    type Surface = VulkanWindow;
    type Streams = S;
}

pub trait RendererBackend: Backend<Surface = VulkanWindow, Streams: RendererStreams> {
    fn start<C: SurfaceControl>(self, setup: RendererSetup) -> StartResult<C, Self::Streams>;
}

impl<P: PresentationBackend + Send + 'static, S: RendererStreams> RendererBackend
    for VulkanBackend<P, S>
{
    fn start<C: SurfaceControl>(self, setup: RendererSetup) -> StartResult<C, S> {
        RendererState::with_streams(&self.presentation, setup, self.streams)
    }
}
