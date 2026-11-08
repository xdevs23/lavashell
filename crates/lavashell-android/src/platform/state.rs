// SPDX-License-Identifier: GPL-3.0-only
//! A simulation of the renderer state, which draws through the simulated renderer. The stream code of the renderer
//! crate imports the frames of streams and returns them to the streams that produced them.

use super::{AospPresentation, SurfaceTarget, VulkanRenderer, surface_target};
use crate::NativeWindow;
use lavashell_core::prelude::{OutputInfo, OutputRequest, Rect, SceneSnapshot};
use lavashell_core::surface::SurfaceId;
use lavashell_renderer::text::Shaper;
use lavashell_renderer::{
    DrawError, Drawn, FrameDevice, FrameError, FrameImporter, FrameSource, Latch, LatchedFrames,
    Painter, PresentFrame, RendererSetup, StartError, StateError, Stream, StreamBackend,
    SurfaceAlpha, SurfaceControl, SurfaceFrame, skia,
};
use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::time::Instant;

struct AttachedSurface<C> {
    target: SurfaceTarget,
    control: C,
}

pub(crate) struct RendererState<C, S> {
    painter: Painter,
    surfaces: BTreeMap<SurfaceId, AttachedSurface<C>>,
    gpu: VulkanRenderer,
    streams: S,
}

impl<C: SurfaceControl, S: StreamBackend<Gpu = VulkanRenderer>> RendererState<C, S> {
    pub fn with_streams(
        backend: &AospPresentation,
        setup: RendererSetup,
        _start_without_scene_source: Instant,
        streams: impl FnOnce(&VulkanRenderer) -> Result<S, S::Error>,
    ) -> Result<Self, StartError<S::Error>> {
        let RendererSetup { fonts, .. } = setup;
        let gpu = VulkanRenderer::new(backend).map_err(StateError::CreateRenderer)?;
        let painter = gpu.painter(&fonts).map_err(StateError::CreatePainter)?;
        let streams = streams(&gpu).map_err(StartError::Streams)?;
        Ok(Self {
            painter,
            surfaces: BTreeMap::new(),
            gpu,
            streams,
        })
    }

    pub fn retire(&mut self, surface: SurfaceId) -> Option<C> {
        self.surfaces
            .remove(&surface)
            .map(|attached| attached.control)
    }

    pub fn control(&self, surface: SurfaceId) -> Option<&C> {
        self.surfaces
            .get(&surface)
            .map(|attached| &attached.control)
    }

    pub fn request_output(
        &mut self,
        surface: SurfaceId,
        request: OutputRequest,
    ) -> Result<(), StateError> {
        let attached = self.surfaces.get_mut(&surface);
        let target = &mut attached.ok_or(StateError::UnknownSurface(surface))?.target;
        target.request_output(request);
        Ok(())
    }

    pub fn output(&self, surface: SurfaceId) -> Option<OutputInfo> {
        (self.surfaces.get(&surface)).and_then(|attached| attached.target.output())
    }

    pub fn device_name(&self) -> &str {
        &self.gpu.device_name
    }

    pub fn supports_presentation_time(&self) -> bool {
        self.gpu.supports_presentation_time()
    }

    pub fn shaper(&mut self) -> &mut Shaper {
        self.painter.shaper()
    }

    /// Draws `snapshot` into the simulated window of `surface`. The whole frame is painted, because the simulated
    /// window clears its image before every frame.
    pub fn draw(
        &mut self,
        surface: SurfaceId,
        frame: PresentFrame<'_>,
        snapshot: &SceneSnapshot,
    ) -> Result<Drawn, DrawError<C::Error, S::Error>> {
        let size = (frame.viewport.size.buffer_size(frame.viewport.density))
            .map_err(StateError::BufferSize)?;
        let shown = snapshot.at(frame.time.presentation);
        let attached =
            (self.surfaces.get_mut(&surface)).ok_or(StateError::UnknownSurface(surface))?;
        (attached.control.prepare(&frame)).map_err(FrameError::Control)?;
        let draw_frame =
            |device: FrameDevice<'_, VulkanRenderer>| -> Result<Drawn, FrameError<C::Error>> {
                let FrameDevice {
                    gpu,
                    painter,
                    streams,
                } = device;
                let mut painted = Ok(());
                let paint_frame = |canvas: &skia::Canvas| {
                    attached.control.before_present(&frame);
                    let whole = SurfaceFrame {
                        tree: shown,
                        density: frame.viewport.density,
                        damage: Some(Rect::from_size(frame.viewport.size)),
                        clear: skia::Color::TRANSPARENT,
                        scene: None,
                        shape: None,
                        streams,
                    };
                    painted = painter.paint_surface(canvas, whole);
                };
                let outcome =
                    (gpu.present(&mut attached.target, size, frame.present_at, paint_frame))
                        .map_err(FrameError::Draw)
                        .and_then(|outcome| painted.map(|()| outcome).map_err(FrameError::Paint))?;
                attached.control.after_present(outcome);
                Ok(Drawn {
                    outcome,
                    stats: painter.render_stats(),
                    gpu: None,
                    output: attached.target.output(),
                    scene: None,
                })
            };
        let (drawn, streams) =
            (self.streams).draw(&mut self.gpu, &mut self.painter, shown, draw_frame);
        DrawError::combine(drawn, streams)
    }
}

impl<C: SurfaceControl, I: FrameImporter> RendererState<C, LatchedFrames<I>> {
    pub fn latch<F>(&mut self, stream: &mut Stream<F>) -> Result<Latch, I::Error>
    where
        F: FrameSource<Frame = I::Frame>,
        I::Error: From<F::Error>,
    {
        self.streams.latch(&self.painter, stream)
    }
}

/// Attaches `window` to `state` as `surface`, with the window as the control of the surface.
pub(crate) fn attach_window<S>(
    state: &mut RendererState<NativeWindow, S>,
    surface: SurfaceId,
    window: NativeWindow,
    alpha: SurfaceAlpha,
) -> Result<(), StateError> {
    let Entry::Vacant(entry) = state.surfaces.entry(surface) else {
        return Err(StateError::AttachedTwice(surface));
    };
    let target = surface_target(&state.gpu, &window, alpha).map_err(StateError::CreateTarget)?;
    let _attached = entry.insert(AttachedSurface {
        target,
        control: window,
    });
    Ok(())
}
