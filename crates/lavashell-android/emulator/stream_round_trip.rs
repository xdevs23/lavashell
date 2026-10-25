// SPDX-License-Identifier: GPL-3.0-only
use crate::error::NativeMethodError;
use lavashell_android::{
    AndroidRenderer, BufferSizes, Latch, NativeWindow, StreamFormat, SurfaceStream,
};
use lavashell_core::prelude::*;
use lavashell_core::scene::tree::{Change, Content, Group, Node, Packet, SceneNodeId, Slot};
use lavashell_renderer::vulkan::FormatProperties;
use lavashell_renderer::{
    Fonts, FrameOutcome, PresentationPolicy, SurfaceAlpha, SurfaceTiming, skia,
};
use ndk::hardware_buffer::{HardwareBuffer, HardwareBufferUsage};
use ndk::media::image_reader::{AcquireResult, Image, ImageFormat, ImageReader};
use rustix::event::{PollFd, PollFlags, poll};
use rustix::io::Errno;
use std::os::fd::BorrowedFd;
use std::sync::mpsc;
use std::time::Duration;

/// One round trip of a color. The activity sends a frame filled with a known color into the Surface of `stream`.
/// `latch` makes the newest frame of the stream the one that the renderer draws. `render` draws that frame over `pane`
/// into `target` and reads the color back from `target`. The activity compares that color with the color it sent.
pub struct StreamRoundTrip {
    // Declared before `target`, so the renderer detaches from the window of the target ImageReader before that reader
    // is deleted.
    renderer: AndroidRenderer,
    stream: SurfaceStream,
    target: TargetReader,
    pane: skia::IRect,
    arrived: mpsc::Receiver<()>,
    wakes: u32,
    image: Option<StreamImage>,
}

impl StreamRoundTrip {
    pub fn new(sizes: BufferSizes, format: StreamFormat) -> Result<Self, NativeMethodError> {
        let mut renderer = AndroidRenderer::new(&Fonts::new()).map_err(NativeMethodError::SetUp)?;
        let mut stream = SurfaceStream::new(sizes, format).map_err(NativeMethodError::SetUp)?;
        let (notify, arrived) = mpsc::channel();
        stream.on_frame(move || match notify.send(()) {
            // A notification sent after the round trip is dropped has no receiver and is ignored.
            Ok(()) | Err(mpsc::SendError(())) => {}
        });
        let (width, height) =
            (sizes.default.signed()).ok_or(NativeMethodError::TargetSize(sizes.default))?;
        let target = TargetReader::new(width, height)?;
        let window = (target.reader.window()).map_err(NativeMethodError::CreateTarget)?;
        let timing = SurfaceTiming::new(PresentationPolicy::Automatic);
        (renderer.attach(NativeWindow::new(window, timing), SurfaceAlpha::Opaque))
            .map_err(NativeMethodError::SetUp)?;
        Ok(Self {
            renderer,
            stream,
            target,
            pane: skia::IRect::from_wh(width, height),
            arrived,
            wakes: 0,
            image: None,
        })
    }

    /// The window of the Surface that the producer draws into.
    pub fn producer_window(&self) -> &ndk::native_window::NativeWindow {
        self.stream.native_window()
    }

    pub fn resize(&mut self, sizes: BufferSizes) -> Result<(), NativeMethodError> {
        self.stream.resize(sizes).map_err(NativeMethodError::Resize)
    }

    /// Reads the format properties that the Vulkan device of the renderer returns for `buffer`.
    pub fn read_buffer_format(
        &self,
        buffer: &HardwareBuffer,
    ) -> Result<FormatProperties, NativeMethodError> {
        (self.renderer.buffer_format(buffer)).map_err(NativeMethodError::BufferFormat)
    }

    pub fn drawn_size(&self) -> Option<PixelSize> {
        self.stream.drawn_size()
    }

    /// How often the round trip has received a notification of a frame from the stream.
    pub fn wakes(&self) -> u32 {
        self.wakes
    }

    /// Blocks until the stream notifies the round trip of a frame.
    pub fn await_wake(&mut self) -> Result<(), NativeMethodError> {
        (self.arrived.recv()).map_err(NativeMethodError::AwaitNotification)?;
        self.wakes = self.wakes.saturating_add(1);
        Ok(())
    }

    /// Destroys the round trip and returns how many notifications of a frame the stream sent that were not received.
    pub fn close(self) -> u32 {
        let Self {
            renderer,
            stream,
            arrived,
            ..
        } = self;
        // The stream sends the notifications, so `arrived` ends only once the stream is dropped.
        drop((renderer, stream));
        arrived.iter().fold(0, |count, ()| count.saturating_add(1))
    }

    /// Blocks until the stream latches a frame that it has not latched before.
    pub fn latch(&mut self) -> Result<(), NativeMethodError> {
        loop {
            match (self.renderer.latch(&mut self.stream)).map_err(NativeMethodError::Latch)? {
                Latch::Frame(image) => {
                    self.image = Some(image);
                    return Ok(());
                }
                Latch::Rejected(rejection) => {
                    return Err(NativeMethodError::FrameRejected(rejection));
                }
                Latch::Unchanged => {}
            }
            match self.stream.producer_fence() {
                Some(fence) => wait_until_signalled(fence)?,
                None => self.await_wake()?,
            }
        }
    }

    /// Draws the latched frame over the pane and returns the color of the pixel at the center of the pane.
    pub fn render(&mut self) -> Result<skia::Color, NativeMethodError> {
        let leaf = Change::Insert {
            id: SceneNodeId(1),
            parent: None,
            index: 0,
            node: Node::Leaf(self.build_pane_content()?),
        };
        self.present(&build_scene(vec![leaf])?, skia::Color::TRANSPARENT)?;
        let center = find_center(self.pane);
        self.target
            .read_next_frame(|frame| read_pixel(frame, center))
    }

    /// Draws the latched frame over black inside a group that fades the frame to `opacity` and moves the frame right by
    /// the width of the left half of the pane. Returns the colors of the pixels at the centers of the left half and of
    /// the right half. The moved frame leaves the left half black and covers the right half.
    pub fn render_grouped(&mut self, opacity: f64) -> Result<[skia::Color; 2], NativeMethodError> {
        let pane = self.pane;
        let middle = find_center(pane).x;
        let left = skia::IRect::new(pane.left, pane.top, middle, pane.bottom);
        let right = skia::IRect::new(middle, pane.top, pane.right, pane.bottom);
        let group = Group {
            transform: Slot::Value(Affine::translate(Point::new(left.width().dp(), 0.dp()))),
            opacity: Slot::Value(opacity),
            clip: None,
        };
        let (group_id, leaf_id) = (SceneNodeId(1), SceneNodeId(2));
        let changes = vec![
            Change::Insert {
                id: group_id,
                parent: None,
                index: 0,
                node: Node::Group(group),
            },
            Change::Insert {
                id: leaf_id,
                parent: Some(group_id),
                index: 0,
                node: Node::Leaf(self.build_pane_content()?),
            },
        ];
        self.present(&build_scene(changes)?, skia::Color::BLACK)?;
        let (left, right) = (find_center(left), find_center(right));
        self.target
            .read_next_frame(|frame| Ok([read_pixel(frame, left)?, read_pixel(frame, right)?]))
    }

    fn build_pane_content(&self) -> Result<Content, NativeMethodError> {
        let image = self.image.ok_or(NativeMethodError::NothingLatched)?;
        let pane = Rect::new(
            self.pane.left.dp(),
            self.pane.top.dp(),
            self.pane.width().dp(),
            self.pane.height().dp(),
        );
        let commands = [DrawCommand::StreamImage {
            image,
            destination: pane,
            frame: pane,
        }];
        Ok(Content::Packet(Packet::new(commands.into())))
    }

    /// Renders `scene` until the renderer shows the frame in the target `ImageReader`.
    fn present(&mut self, scene: &SceneTree, clear: skia::Color) -> Result<(), NativeMethodError> {
        let mut outcome = self.render_once(scene, clear)?;
        if outcome == FrameOutcome::Stale {
            // The swapchain no longer fits the target ImageReader. The next render replaces the swapchain.
            outcome = self.render_once(scene, clear)?;
        }
        match outcome {
            FrameOutcome::Shown => Ok(()),
            FrameOutcome::Pending | FrameOutcome::Stale => {
                Err(NativeMethodError::NotShown(outcome))
            }
        }
    }

    fn render_once(
        &mut self,
        scene: &SceneTree,
        clear: skia::Color,
    ) -> Result<FrameOutcome, NativeMethodError> {
        (self
            .renderer
            .render(scene.at(Duration::ZERO), Density::ONE, clear))
        .map_err(NativeMethodError::Render)
    }
}

/// The `ImageReader` into which the renderer draws and from which the round trip reads the drawn colors back.
struct TargetReader {
    reader: ImageReader,
    presented: mpsc::Receiver<()>,
}

impl TargetReader {
    fn new(width: i32, height: i32) -> Result<Self, NativeMethodError> {
        let usage = HardwareBufferUsage::CPU_READ_OFTEN | HardwareBufferUsage::GPU_COLOR_OUTPUT;
        // `read_next_frame` releases each frame before it acquires the next one.
        let acquired_frames = 1;
        let mut reader = ImageReader::new_with_usage(
            width,
            height,
            ImageFormat::RGBA_8888,
            usage,
            acquired_frames,
        )
        .map_err(NativeMethodError::CreateTarget)?;
        let (notify, presented) = mpsc::channel();
        reader
            .set_image_listener(Box::new(move |_| match notify.send(()) {
                Ok(()) | Err(mpsc::SendError(())) => {}
            }))
            .map_err(NativeMethodError::CreateTarget)?;
        Ok(Self { reader, presented })
    }

    /// Waits for the next frame presented to the reader and passes that frame to `read`, which returns before the
    /// frame is released.
    fn read_next_frame<T>(
        &mut self,
        read: impl FnOnce(&Image) -> Result<T, NativeMethodError>,
    ) -> Result<T, NativeMethodError> {
        (self.presented.recv()).map_err(NativeMethodError::AwaitNotification)?;
        match (self.reader.acquire_next_image()).map_err(NativeMethodError::ReadTarget)? {
            AcquireResult::Image(frame) => read(&frame),
            AcquireResult::NoBufferAvailable | AcquireResult::MaxImagesAcquired => {
                Err(NativeMethodError::NoTargetFrame)
            }
        }
    }
}

fn build_scene(changes: Vec<Change>) -> Result<SceneTree, NativeMethodError> {
    let mut scene = SceneTree::default();
    scene
        .show(SceneUpdate { changes })
        .map_err(NativeMethodError::BuildScene)?;
    Ok(scene)
}

fn wait_until_signalled(fence: BorrowedFd<'_>) -> Result<(), NativeMethodError> {
    let mut fences = [PollFd::new(&fence, PollFlags::IN)];
    loop {
        match poll(&mut fences, None) {
            Ok(0) | Err(Errno::INTR) => {}
            Ok(_) => return Ok(()),
            Err(error) => return Err(NativeMethodError::WaitForProducer(error)),
        }
    }
}

fn find_center(area: skia::IRect) -> skia::IPoint {
    skia::IPoint::new(area.left + area.width() / 2, area.top + area.height() / 2)
}

/// Reads the color of the pixel at `at` in `frame`, an `RGBA_8888` frame with one plane.
fn read_pixel(frame: &Image, at: skia::IPoint) -> Result<skia::Color, NativeMethodError> {
    let row = (frame.plane_row_stride(0)).map_err(NativeMethodError::ReadTarget)?;
    let step = (frame.plane_pixel_stride(0)).map_err(NativeMethodError::ReadTarget)?;
    let data = frame.plane_data(0).map_err(NativeMethodError::ReadTarget)?;
    let [Ok(x), Ok(y), Ok(row), Ok(step)] = [at.x, at.y, row, step].map(usize::try_from) else {
        return Err(NativeMethodError::PixelOutside(at));
    };
    let Some(&[red, green, blue, alpha, ..]) = data.get(y * row + x * step..) else {
        return Err(NativeMethodError::PixelOutside(at));
    };
    Ok(skia::Color::from_argb(alpha, red, green, blue))
}
