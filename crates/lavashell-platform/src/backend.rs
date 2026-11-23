// SPDX-License-Identifier: GPL-3.0-only
use crate::{SurfaceControl, SurfaceRenderer};
use lavashell_renderer::{DamageMode, Fonts, FrameTiming, Painter};

pub trait Backend: Send + 'static {
    type Surface: Send + 'static;
    type Streams: 'static;
}

pub trait RendererBackend: Backend {
    type Renderer<C: SurfaceControl>: SurfaceRenderer<C, Self::Surface, Self::Streams>;
    type StartError: std::error::Error + Send + 'static;

    fn shows_scene(&self) -> bool;

    fn start<C: SurfaceControl>(
        self,
        setup: RendererSetup,
    ) -> Result<Self::Renderer<C>, Self::StartError>;
}

pub trait HeadlessRendererBackend: RendererBackend + Sized {
    fn create() -> Option<Self>;

    fn create_surface() -> Self::Surface;
}

pub type PainterSetup = Box<dyn FnOnce(&mut Painter) + Send>;

pub struct RendererSetup {
    pub fonts: Fonts,
    pub painter: Option<PainterSetup>,
    pub frame_timing: FrameTiming,
    pub damage: DamageMode,
}

impl RendererSetup {
    pub fn painter_without_device(self) -> Result<Painter, lavashell_renderer::Error> {
        let mut painter = Painter::without_device(&self.fonts)?;
        if let Some(configure) = self.painter {
            configure(&mut painter);
        }
        Ok(painter)
    }
}

#[cfg(test)]
mod tests;
