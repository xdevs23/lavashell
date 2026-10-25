// SPDX-License-Identifier: GPL-3.0-only
//! Simulations of the renderer and of the parts of the ndk crate called by the AOSP code, for the tests. The
//! simulations have the names and the signatures of `platform.rs`.

mod image;
mod importer;
pub mod ndk;
mod reader;
mod test_error;

pub(crate) use image::{HardwareBufferFormat, HardwareBufferUsage};
pub(crate) use importer::{
    Completion, HardwareBufferImport, HardwareBufferImporter, HardwareBufferProperties,
    ReleaseError, Sampling,
};
use lavashell_core::prelude::{OutputInfo, OutputRequest, PixelSize};
use lavashell_renderer::Error as RendererError;
use lavashell_renderer::{
    Fonts, FrameOutcome, Painter, PresentationPolicy, SurfaceAlpha, SurfaceTiming, UniformRange,
    skia,
};
use reader::Queue;
pub(crate) use reader::{Buffer, Event as StreamEvent, MEDIA, Producer, Writing};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
pub(crate) use test_error::TestError;

/// The simulated GPU of the test running on this thread, shared by the renderer and everything it creates.
#[derive(Default)]
pub(crate) struct Gpu {
    /// The device is lost, so every call into it fails.
    pub lost: Cell<bool>,
    /// The next export of a release fence fails.
    pub fails_release: Cell<bool>,
    /// Every submit of a fence for the completion of the GPU work fails.
    pub fails_completion: Cell<bool>,
    pub busy: Cell<bool>,
}

impl Gpu {
    /// Fails once the device is lost, as the renderer does after Skia abandons its context.
    fn reachable(&self) -> Result<(), RendererError> {
        if self.lost.get() {
            return Err(RendererError::SkiaAbandoned);
        }
        Ok(())
    }
}

thread_local! {
    pub(crate) static GPU: Rc<Gpu> = Rc::default();
}

pub(crate) const SIDE: i32 = 16;
pub(crate) const SURFACE_SIZE: PixelSize = PixelSize::new(SIDE.unsigned_abs(), SIDE.unsigned_abs());

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Event {
    NativeAcquired,
    Attached,
    Acquired,
    Presented(Option<u64>),
    TargetDropped,
    NativeReleased,
}

#[derive(Default)]
pub(crate) struct NativeState {
    events: RefCell<Vec<Event>>,
    pending: Cell<bool>,
    pixels: RefCell<Vec<u8>>,
    explicit: Cell<bool>,
}

pub(crate) struct NativeSurface(Rc<NativeState>);

impl NativeSurface {
    pub fn new() -> Self {
        Self(Rc::new(NativeState::default()))
    }

    /// A native window of the Surface with a new reference, as AOSP acquires one.
    pub fn window(&self) -> NativeWindow {
        self.0.events.borrow_mut().push(Event::NativeAcquired);
        NativeWindow {
            state: self.0.clone(),
            queue: None,
        }
    }

    pub fn references(&self) -> usize {
        Rc::strong_count(&self.0)
    }

    pub fn events(&self) -> Vec<Event> {
        self.0.events.borrow().clone()
    }

    pub fn clear_events(&self) {
        self.0.events.borrow_mut().clear();
    }

    pub fn last_event(&self) -> Option<Event> {
        self.0.events.borrow().last().copied()
    }

    pub fn acquisitions(&self) -> usize {
        self.0
            .events
            .borrow()
            .iter()
            .filter(|event| **event == Event::Acquired)
            .count()
    }

    pub fn presentations(&self) -> Vec<Option<u64>> {
        self.0
            .events
            .borrow()
            .iter()
            .filter_map(|event| match event {
                Event::Presented(time) => Some(*time),
                _ => None,
            })
            .collect()
    }

    pub fn make_next_acquisition_pending(&self) {
        self.0.pending.set(true);
    }

    /// The RGBA bytes of the pixel at `x`, `y`.
    pub fn pixel(&self, x: i32, y: i32) -> Result<[u8; 4], TestError> {
        let pixels = self.0.pixels.borrow();
        let (rgba, partial) = pixels.as_chunks::<4>();
        assert!(partial.is_empty(), "the pixels hold whole RGBA pixels");
        let index = usize::try_from(y * SIDE + x).ok();
        (index.and_then(|index| rgba.get(index)))
            .copied()
            .ok_or(TestError::PixelOutside { x, y })
    }
}

/// The native window of the ndk crate, simulated for the Surface of a display and for the Surface of a reader.
pub struct NativeWindow {
    state: Rc<NativeState>,
    queue: Option<Rc<Queue>>,
}

impl NativeWindow {
    pub(crate) fn of_reader(state: &Rc<NativeState>, queue: &Rc<Queue>) -> Self {
        Self {
            state: state.clone(),
            queue: Some(queue.clone()),
        }
    }

    /// The queue of the reader, or `None` for the window of the simulated display.
    pub(crate) fn queue(&self) -> Option<&Rc<Queue>> {
        self.queue.as_ref()
    }

    /// The default size of the buffers of a reader, or the size of the simulated display.
    fn lengths(&self) -> (i32, i32) {
        self.queue
            .as_ref()
            .map_or((SIDE, SIDE), |queue| queue.lengths)
    }

    pub fn width(&self) -> i32 {
        self.lengths().0
    }

    pub fn height(&self) -> i32 {
        self.lengths().1
    }
}

impl Clone for NativeWindow {
    fn clone(&self) -> Self {
        self.state.events.borrow_mut().push(Event::NativeAcquired);
        Self {
            state: self.state.clone(),
            queue: self.queue.clone(),
        }
    }
}

impl Drop for NativeWindow {
    fn drop(&mut self) {
        self.state.events.borrow_mut().push(Event::NativeReleased);
    }
}

impl PartialEq for NativeWindow {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.state, &other.state)
    }
}

pub(crate) struct SurfaceTarget {
    window: NativeWindow,
    timing: SurfaceTiming,
    request: OutputRequest,
    /// The output of the frame presented last.
    output: Option<OutputInfo>,
}

impl SurfaceTarget {
    pub fn output(&self) -> Option<OutputInfo> {
        self.output
    }

    pub fn request_output(&mut self, request: OutputRequest) {
        self.request = request;
    }

    /// The output of a frame presented into the simulated window. The window contains sRGB pixels with 8 bits per
    /// channel.
    fn presented_output(&self) -> OutputInfo {
        let shown = OutputRequest::default();
        OutputInfo {
            requested: self.request,
            profile: shown.profile,
            color_bits: shown.precision.bits(),
            alpha_bits: shown.precision.bits(),
            floating_point: false,
            fallback: self.request != shown,
        }
    }
}

impl Drop for SurfaceTarget {
    fn drop(&mut self) {
        self.window
            .state
            .events
            .borrow_mut()
            .push(Event::TargetDropped);
    }
}

pub(crate) struct VulkanRenderer {
    pub device_name: String,
    gpu: Rc<Gpu>,
    /// The range of uniforms that a runtime effect may use on the simulated device.
    uniforms: UniformRange,
    /// The simulated device has `VK_GOOGLE_display_timing`.
    display_timing: bool,
    /// The image that every frame is drawn into before its pixels are copied to the window.
    frame: skia::Surface,
}

impl VulkanRenderer {
    pub fn for_android() -> Result<Self, RendererError> {
        let size = (SURFACE_SIZE.width, SURFACE_SIZE.height);
        let frame = skia::surfaces::raster(&frame_info(), None, None)
            .ok_or(RendererError::OffscreenSurface(size))?;
        Ok(Self {
            device_name: "simulated Vulkan boundary".into(),
            gpu: GPU.with(Rc::clone),
            uniforms: UniformRange::VULKAN_REQUIRED,
            display_timing: true,
            frame,
        })
    }

    pub fn painter(&self, fonts: &Fonts) -> Result<Painter, RendererError> {
        Painter::new(fonts, self.uniforms)
    }

    pub fn supports_presentation_time(&self) -> bool {
        self.display_timing
    }

    pub fn hardware_buffer_importer(&self) -> Result<HardwareBufferImporter, RendererError> {
        self.gpu.reachable()?;
        Ok(HardwareBufferImporter::new(&self.gpu))
    }

    pub fn native_window_target(
        &self,
        window: NativeWindow,
        _: SurfaceAlpha,
        timing: SurfaceTiming,
    ) -> Result<SurfaceTarget, RendererError> {
        self.gpu.reachable()?;
        window.state.events.borrow_mut().push(Event::Attached);
        Ok(SurfaceTarget {
            window,
            timing,
            request: OutputRequest::default(),
            output: None,
        })
    }

    pub fn draw(
        &mut self,
        target: &mut SurfaceTarget,
        size: (u32, u32),
        draw: impl FnOnce(&skia::Canvas),
    ) -> Result<FrameOutcome, RendererError> {
        self.present(target, size, None, draw)
    }

    pub fn draw_at(
        &mut self,
        target: &mut SurfaceTarget,
        size: (u32, u32),
        time: u64,
        draw: impl FnOnce(&skia::Canvas),
    ) -> Result<FrameOutcome, RendererError> {
        self.present(target, size, Some(time), draw)
    }

    /// Applies the timestamp rule of the renderer: once a frame on a window had an explicit timestamp, every later
    /// frame needs one. The renderer records that state privately, so the simulated window records it in `explicit`.
    fn present(
        &mut self,
        target: &mut SurfaceTarget,
        size: (u32, u32),
        time: Option<u64>,
        draw: impl FnOnce(&skia::Canvas),
    ) -> Result<FrameOutcome, RendererError> {
        self.gpu.reachable()?;
        if let Some(time) = time.filter(|&time| time == 0 || i64::try_from(time).is_err()) {
            return Err(RendererError::PresentationTimeRange(time));
        }
        if size.0 == 0 || size.1 == 0 {
            return Ok(FrameOutcome::Pending);
        }
        let native = &target.window.state;
        let explicit =
            target.timing.policy() == PresentationPolicy::Explicit || native.explicit.get();
        if explicit && time.is_none() {
            return Err(RendererError::ExplicitTimestampRequired);
        }
        native.events.borrow_mut().push(Event::Acquired);
        if native.pending.replace(false) {
            return Ok(FrameOutcome::Pending);
        }
        draw(self.frame.canvas().clear(skia::Color::TRANSPARENT));
        let info = frame_info();
        let mut pixels = vec![0; info.compute_min_byte_size()];
        assert!(
            self.frame
                .read_pixels(&info, &mut pixels, info.min_row_bytes(), (0, 0))
        );
        *native.pixels.borrow_mut() = pixels;
        if time.is_some() {
            native.explicit.set(true);
        }
        native.events.borrow_mut().push(Event::Presented(time));
        target.output = Some(target.presented_output());
        Ok(FrameOutcome::Shown)
    }
}

/// The pixel format and the size of the simulated display: `SIDE` × `SIDE` premultiplied RGBA pixels in sRGB.
fn frame_info() -> skia::ImageInfo {
    skia::ImageInfo::new(
        (SIDE, SIDE),
        skia::ColorType::RGBA8888,
        skia::AlphaType::Premul,
        skia::ColorSpace::new_srgb(),
    )
}
