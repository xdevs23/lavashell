// SPDX-License-Identifier: GPL-3.0-only

mod image;
mod importer;
pub mod ndk;
mod reader;
mod simulation;
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
use lavashell_platform::FrameOutcome;
pub(crate) use lavashell_renderer::Sampling;
use lavashell_renderer::{Fonts, Painter, UniformRange, skia};
use lavashell_vulkan::{Instance, Window};
use lavashell_vulkan_renderer::{
    Error as RendererError, HeadlessPresentation, VulkanBackend, VulkanWindow,
};
use reader::Queue;
pub(crate) use reader::{Buffer, Event as StreamEvent, Producer, Writing};
pub(crate) use simulation::Simulation;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
pub(crate) use test_error::TestError;

#[derive(Default)]
pub(crate) struct Gpu {
    pub lost: Cell<bool>,
    pub fails_release: Cell<bool>,
    pub fails_completion: Cell<bool>,
    pub busy: Cell<bool>,
    pub sampler_ycbcr_conversion: Cell<bool>,
    pub pending: Cell<bool>,
}

impl Gpu {
    fn reachable(&self) -> Result<(), RendererError> {
        if self.lost.get() {
            return Err(RendererError::SkiaAbandoned);
        }
        Ok(())
    }
}

thread_local! {
    pub(crate) static GPU: RefCell<Rc<Gpu>> = RefCell::default();
}

const WINDOW_SIZE_FAILURE: (i32, i32) = (-1, -1);

pub(crate) const SIDE: i32 = 16;
pub(crate) const SURFACE_SIZE: PixelSize = PixelSize::new(SIDE_LENGTH, SIDE_LENGTH);
const SIDE_LENGTH: u32 = {
    assert!(SIDE >= 0);
    SIDE.cast_unsigned()
};

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

#[derive(Clone)]
pub struct NativeWindow(Surface);

#[derive(Clone)]
enum Surface {
    Display(Arc<Mutex<(i32, i32)>>),
    Reader { key: usize, size: PixelSize },
}

impl NativeWindow {
    pub(crate) fn of_reader(queue: &Queue) -> Self {
        Self(Surface::Reader {
            key: queue.key,
            size: queue.size,
        })
    }

    pub(crate) fn reader(&self) -> Option<usize> {
        match self.0 {
            Surface::Reader { key, .. } => Some(key),
            Surface::Display(_) => None,
        }
    }

    fn lengths(&self) -> (i32, i32) {
        match &self.0 {
            Surface::Display(lengths) => *lock_ignoring_poison(lengths),
            Surface::Reader { size, .. } => size.signed().unwrap_or(WINDOW_SIZE_FAILURE),
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

pub(crate) fn vulkan_window(_: &crate::NativeWindow) -> VulkanWindow {
    Arc::new(SimulatedWindow)
}

pub(crate) type AospBackend = VulkanBackend<HeadlessPresentation, crate::AospStreams>;

pub struct VulkanRenderer {
    gpu: Rc<Gpu>,
    uniform_range: UniformRange,
    frame: skia::Surface,
    pixels: Vec<u8>,
}

impl VulkanRenderer {
    pub fn new() -> Result<Self, RendererError> {
        let frame = skia::surfaces::raster(&frame_info(), None, None)
            .ok_or(RendererError::OffscreenSurface(SURFACE_SIZE))?;
        Ok(Self {
            gpu: GPU.with_borrow(Rc::clone),
            uniform_range: UniformRange::VULKAN_REQUIRED,
            frame,
            pixels: Vec::new(),
        })
    }

    pub fn painter(&self, fonts: &Fonts) -> Result<Painter, lavashell_renderer::Error> {
        Painter::new(fonts, self.uniform_range)
    }

    pub fn foreign_importer(&self) -> Result<ForeignImporter, RendererError> {
        self.gpu.reachable()?;
        Ok(ForeignImporter::new(&self.gpu))
    }

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

    pub(crate) fn pixel(&self, x: i32, y: i32) -> Result<[u8; 4], TestError> {
        let (rgba, partial) = self.pixels.as_chunks::<4>();
        assert!(partial.is_empty(), "the pixels contain whole RGBA pixels");
        let (0..SIDE, 0..SIDE) = (x, y) else {
            return Err(TestError::PixelOutside { x, y });
        };
        (usize::try_from(y * SIDE + x).ok())
            .and_then(|index| rgba.get(index))
            .copied()
            .ok_or(TestError::PixelOutside { x, y })
    }
}

fn frame_info() -> skia::ImageInfo {
    skia::ImageInfo::new(
        (SIDE, SIDE),
        skia::ColorType::RGBA8888,
        skia::AlphaType::Premul,
        skia::ColorSpace::new_srgb(),
    )
}
