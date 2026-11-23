// SPDX-License-Identifier: GPL-3.0-only
use crate::vulkan::{HeadlessPresentation, PresentationBackend, Window};
use crate::{NoStreams, RendererState, RendererStreams, SceneSetup, StartError};
use lavashell_platform::{
    Backend, HeadlessRendererBackend, RendererBackend, RendererSetup, SurfaceControl,
};
use lavashell_renderer::StreamBackend;
use lavashell_vulkan::HeadlessWindow;
use std::sync::Arc;

type StreamSetup<S> =
    Box<dyn FnOnce(&<S as StreamBackend>::Gpu) -> Result<S, <S as StreamBackend>::Error> + Send>;

pub type VulkanWindow = Arc<dyn Window>;

pub struct VulkanBackend<P, S: StreamBackend = NoStreams> {
    presentation: P,
    streams: StreamSetup<S>,
    scene: Option<SceneSetup>,
}

impl<P, S: StreamBackend> VulkanBackend<P, S> {
    pub fn new(
        presentation: P,
        streams: impl FnOnce(&S::Gpu) -> Result<S, S::Error> + Send + 'static,
    ) -> Self {
        Self {
            presentation,
            streams: Box::new(streams),
            scene: None,
        }
    }

    #[must_use]
    pub fn with_scene(self, scene: SceneSetup) -> Self {
        Self {
            scene: Some(scene),
            ..self
        }
    }
}

impl<P> VulkanBackend<P> {
    pub fn without_streams(presentation: P) -> Self {
        Self::new(presentation, |_| Ok(NoStreams::default()))
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

impl HeadlessRendererBackend for HeadlessBackend {
    fn create() -> Self {
        Self::with_surfaces()
    }

    fn create_surface() -> VulkanWindow {
        Arc::new(HeadlessWindow)
    }
}

impl<P: PresentationBackend + Send + 'static, S: StreamBackend + 'static> Backend
    for VulkanBackend<P, S>
{
    type Surface = VulkanWindow;
    type Streams = S;
}

impl<P: PresentationBackend + Send + 'static, S: RendererStreams> RendererBackend
    for VulkanBackend<P, S>
{
    type Renderer<C: SurfaceControl> = RendererState<C, S>;
    type StartError = StartError<S::Error>;

    fn shows_scene(&self) -> bool {
        self.scene.is_some()
    }

    fn start<C: SurfaceControl>(
        self,
        setup: RendererSetup,
    ) -> Result<RendererState<C, S>, StartError<S::Error>> {
        RendererState::with_streams(&self.presentation, setup, self.scene, self.streams)
    }
}

#[cfg(test)]
mod tests;
