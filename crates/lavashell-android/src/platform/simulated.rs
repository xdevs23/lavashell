// SPDX-License-Identifier: GPL-3.0-only
//! Simulations of the renderer and of the parts of the ndk crate called by the AOSP code, for the tests. The
//! simulations define the names of `platform.rs` that this crate uses. The simulated renderer draws each frame into
//! the pixels of a simulated display.

mod image;
mod importer;
pub mod ndk;
mod reader;
mod test_error;

use ash::khr::android_surface;
use ash::vk;
pub(crate) use image::{HardwareBufferFormat, HardwareBufferUsage};
pub(crate) use importer::{
    Completion, ForeignImport, ForeignImporter, HardwareBufferMemory, PropertiesError,
    ReleaseError, SampledMemory,
};
use lavashell_actor::lock_ignoring_poison;
use lavashell_core::prelude::PixelSize;
use lavashell_renderer::Error as RendererError;
pub(crate) use lavashell_renderer::Sampling;
use lavashell_renderer::{Fonts, FrameOutcome, Painter, UniformRange, skia};
use lavashell_vulkan::{Instance, Window};
use reader::Queue;
pub(crate) use reader::{Buffer, Event as StreamEvent, MEDIA, Producer, Writing};
use std::cell::Cell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
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
    /// The device samples formats through a YCbCr conversion.
    pub sampler_ycbcr_conversion: Cell<bool>,
    /// The next frame finds no image of the display to draw into.
    pub pending: Cell<bool>,
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

/// A Surface of the simulated display, whose width and height in pixels the host can change.
pub(crate) struct DisplaySurface(Arc<Mutex<(i32, i32)>>);

impl DisplaySurface {
    pub fn new(width: i32, height: i32) -> Self {
        Self(Arc::new(Mutex::new((width, height))))
    }

    pub fn window(&self) -> NativeWindow {
        NativeWindow(Surface::Display(self.0.clone()))
    }

    pub fn resize(&self, width: i32, height: i32) {
        *lock_ignoring_poison(&self.0) = (width, height);
    }
}

/// The native window of the ndk crate, simulated for the Surface of a display and for the Surface of a reader. Like
/// the window of the ndk crate, it can be sent to and shared with other threads. A clone refers to the same Surface.
#[derive(Clone)]
pub struct NativeWindow(Surface);

#[derive(Clone)]
enum Surface {
    Display(Arc<Mutex<(i32, i32)>>),
    /// The reader with the key `key` on the thread of the test, and the default size of its buffers.
    Reader {
        key: usize,
        lengths: (i32, i32),
    },
}

impl NativeWindow {
    pub(crate) fn of_reader(queue: &Queue) -> Self {
        Self(Surface::Reader {
            key: queue.key,
            lengths: queue.lengths,
        })
    }

    /// The key of the reader, or `None` for the window of a display.
    pub(crate) fn reader(&self) -> Option<usize> {
        match self.0 {
            Surface::Reader { key, .. } => Some(key),
            Surface::Display(_) => None,
        }
    }

    /// The default size of the buffers of a reader, or the size of a display.
    fn lengths(&self) -> (i32, i32) {
        match &self.0 {
            Surface::Display(lengths) => *lock_ignoring_poison(lengths),
            Surface::Reader { lengths, .. } => *lengths,
        }
    }

    pub fn width(&self) -> i32 {
        self.lengths().0
    }

    pub fn height(&self) -> i32 {
        self.lengths().1
    }
}

impl PartialEq for NativeWindow {
    fn eq(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (Surface::Display(one), Surface::Display(other)) => Arc::ptr_eq(one, other),
            (Surface::Reader { key: one, .. }, Surface::Reader { key: other, .. }) => one == other,
            (Surface::Display(_), Surface::Reader { .. })
            | (Surface::Reader { .. }, Surface::Display(_)) => false,
        }
    }
}

/// The Vulkan window of a native window. The simulated renderer creates no surface from it.
struct SimulatedWindow;

// SAFETY: The trait is unsafe because a `Surface` destroys the handle that `create_surface` returns. This
// `create_surface` returns no handle, so no surface can outlive a native window.
unsafe impl Window for SimulatedWindow {
    fn create_surface(&self, _: &Instance) -> Result<vk::SurfaceKHR, lavashell_vulkan::Error> {
        Err(lavashell_vulkan::Error::MissingExtension(
            android_surface::NAME,
        ))
    }
}

pub(crate) fn vulkan_window(_: &crate::NativeWindow) -> Arc<dyn Window> {
    Arc::new(SimulatedWindow)
}

/// Public because it is the GPU of the public `AospStreams`.
pub struct VulkanRenderer {
    gpu: Rc<Gpu>,
    /// The range of uniforms that a runtime effect may use on the simulated device.
    uniform_range: UniformRange,
    /// The image that every frame is drawn into before its pixels are copied to the display.
    frame: skia::Surface,
    /// The RGBA pixels of the display.
    pixels: Vec<u8>,
}

impl VulkanRenderer {
    pub fn new() -> Result<Self, RendererError> {
        let size = (SURFACE_SIZE.width, SURFACE_SIZE.height);
        let frame = skia::surfaces::raster(&frame_info(), None, None)
            .ok_or(RendererError::OffscreenSurface(size))?;
        Ok(Self {
            gpu: GPU.with(Rc::clone),
            uniform_range: UniformRange::VULKAN_REQUIRED,
            frame,
            pixels: Vec::new(),
        })
    }

    pub fn painter(&self, fonts: &Fonts) -> Result<Painter, RendererError> {
        Painter::new(fonts, self.uniform_range)
    }

    pub fn foreign_importer(&self) -> Result<ForeignImporter, RendererError> {
        self.gpu.reachable()?;
        Ok(ForeignImporter::new(&self.gpu))
    }

    /// Draws a frame with `draw` onto a cleared image and copies its pixels to the display.
    pub fn present(
        &mut self,
        draw: impl FnOnce(&skia::Canvas),
    ) -> Result<FrameOutcome, RendererError> {
        self.gpu.reachable()?;
        if self.gpu.pending.replace(false) {
            return Ok(FrameOutcome::Pending);
        }
        draw(self.frame.canvas().clear(skia::Color::TRANSPARENT));
        let info = frame_info();
        let mut pixels = vec![0; info.compute_min_byte_size()];
        assert!(
            self.frame
                .read_pixels(&info, &mut pixels, info.min_row_bytes(), (0, 0))
        );
        self.pixels = pixels;
        Ok(FrameOutcome::Shown)
    }

    /// The RGBA bytes of the pixel at `x`, `y` of the display.
    pub(crate) fn pixel(&self, x: i32, y: i32) -> Result<[u8; 4], TestError> {
        let (rgba, partial) = self.pixels.as_chunks::<4>();
        assert!(partial.is_empty(), "the pixels contain whole RGBA pixels");
        let index = usize::try_from(y * SIDE + x).ok();
        (index.and_then(|index| rgba.get(index)))
            .copied()
            .ok_or(TestError::PixelOutside { x, y })
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
