// SPDX-License-Identifier: GPL-3.0-only
use crate::error::NativeMethodError;
use crate::pane::{Pane, run_pane};
use lavashell_actor::{Receiver, Sender, mailbox};
use lavashell_android::{
    AospHost, AospStreams, BufferSizes, NativeWindow, StreamFormat, SurfaceStream,
};
use lavashell_core::prelude::*;
use lavashell_renderer::{
    ForeignFormat, Latch, Painter, PresentationPolicy, Stream, SurfaceTiming, skia,
};
use ndk::hardware_buffer::{HardwareBuffer, HardwareBufferRef, HardwareBufferUsage};
use ndk::media::image_reader::{AcquireResult, Image, ImageFormat, ImageReader};
use rustix::event::{PollFd, PollFlags, poll};
use rustix::io::Errno;
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::sync::mpsc;
use std::thread::{self, JoinHandle};

/// One round trip of a color. The activity sends a frame filled with a known color into the Surface of a stream.
/// `latch` selects the newest frame for the pane. `render` draws the pane into the target `ImageReader` and reads the
/// color back. The activity compares that color with the color it sent.
pub struct StreamRoundTrip {
    // Declared before `target` so that the run retires the window before the reader is deleted.
    run: Run,
    target: TargetReader,
    stream: StreamId,
    pane: skia::IRect,
    requested: Sender<Pane>,
    arrived: mpsc::Receiver<()>,
    wakes: u32,
    image: Option<StreamImage>,
}

impl StreamRoundTrip {
    pub fn new(sizes: BufferSizes, format: StreamFormat) -> Result<Self, NativeMethodError> {
        let (width, height) =
            (sizes.default.signed()).ok_or(NativeMethodError::TargetSize(sizes.default))?;
        let (mut target, ended) = TargetReader::new(width, height)?;
        let window = (target.reader.window()).map_err(NativeMethodError::CreateTarget)?;
        let (requested, panes) = mailbox();
        let run = Run::start(Size::new(width.dp(), height.dp()), panes, ended)?;
        let timing = SurfaceTiming::new(PresentationPolicy::Automatic);
        run.host.surface_created(NativeWindow::new(window, timing));
        // The run shows the empty pane once the window opens in the Surface. The stream is attached after that frame,
        // so that a failed attachment closes a window that is shown, which ends the run.
        target.read_next_frame(|_| Ok(()))?;
        let (notify, arrived) = mpsc::channel();
        let stream = run_on_streams(&run.host, move |streams, _| {
            (attach_stream(streams, sizes, format, notify)).map_err(NativeMethodError::SetUp)
        })?;
        Ok(Self {
            run,
            target,
            stream,
            pane: skia::IRect::from_wh(width, height),
            requested,
            arrived,
            wakes: 0,
            image: None,
        })
    }

    /// The window of the Surface that the producer draws into.
    pub fn producer_window(&self) -> Result<ndk::native_window::NativeWindow, NativeMethodError> {
        self.on_stream(|stream| Ok(stream.source().native_window().clone()))
    }

    pub fn resize(&self, sizes: BufferSizes) -> Result<(), NativeMethodError> {
        self.on_stream(move |stream| {
            (stream.source_mut().resize(sizes)).map_err(NativeMethodError::Resize)
        })
    }

    /// Reads the format that the Vulkan device of the renderer reports for `buffer`.
    pub fn read_buffer_format(
        &self,
        buffer: HardwareBufferRef,
    ) -> Result<ForeignFormat, NativeMethodError> {
        let buffer = SentBuffer(buffer);
        run_on_streams(&self.run.host, move |streams, _| {
            (streams.buffer_format(buffer.get())).map_err(NativeMethodError::BufferFormat)
        })
    }

    pub fn drawn_size(&self) -> Result<Option<PixelSize>, NativeMethodError> {
        self.on_stream(|stream| Ok(stream.drawn_size()))
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

    /// Ends the run and returns how many notifications of a frame the stream sent that were not received.
    ///
    /// # Errors
    /// Fails with the failure of the run.
    pub fn close(self) -> Result<u32, NativeMethodError> {
        let Self {
            run,
            target,
            arrived,
            ..
        } = self;
        // The run drops the stream when it ends. `arrived` reaches its final value at that point.
        drop(run);
        target.finish()?;
        Ok(arrived.iter().fold(0, |count, ()| count.saturating_add(1)))
    }

    /// Blocks until the stream latches a frame that it has not latched before.
    pub fn latch(&mut self) -> Result<(), NativeMethodError> {
        loop {
            let stream = self.stream;
            let latched = move |streams: &mut AospStreams, painter: &Painter| {
                latch_once(streams, painter, stream)
            };
            let (latch, producer_fence) = run_on_streams(&self.run.host, latched)?;
            match latch {
                Latch::Frame(image) => {
                    self.image = Some(image);
                    return Ok(());
                }
                Latch::Rejected(rejection) => {
                    return Err(NativeMethodError::FrameRejected(rejection));
                }
                Latch::Unchanged => {}
            }
            match producer_fence {
                Some(fence) => wait_until_signalled(fence.as_fd())?,
                None => self.await_wake()?,
            }
        }
    }

    /// Shows the latched frame over the pane and returns the color of the pixel at the center of the pane.
    pub fn render(&mut self) -> Result<skia::Color, NativeMethodError> {
        let image = self.image.ok_or(NativeMethodError::NothingLatched)?;
        self.show(Pane::Whole(image));
        let center = find_center(self.pane);
        self.target
            .read_next_frame(|frame| read_pixel(frame, center))
    }

    /// Shows the latched frame over black inside a group that fades the frame to `opacity` and moves the frame right by
    /// the width of the left half of the pane. Returns the colors of the pixels at the centers of the left half and of
    /// the right half. The moved frame leaves the left half black and covers the right half.
    pub fn render_grouped(&mut self, opacity: f64) -> Result<[skia::Color; 2], NativeMethodError> {
        let image = self.image.ok_or(NativeMethodError::NothingLatched)?;
        let pane = self.pane;
        let middle = find_center(pane).x;
        let left = skia::IRect::new(pane.left, pane.top, middle, pane.bottom);
        let right = skia::IRect::new(middle, pane.top, pane.right, pane.bottom);
        self.show(Pane::Grouped {
            image,
            opacity,
            shift: left.width().dp(),
        });
        let (left, right) = (find_center(left), find_center(right));
        self.target
            .read_next_frame(|frame| Ok([read_pixel(frame, left)?, read_pixel(frame, right)?]))
    }

    /// Requests `pane` for the next frame, and reports to the run that the display is ready for that frame.
    fn show(&self, pane: Pane) {
        let _ended_run_fails_the_next_read = self.requested.push(pane);
        self.run.host.frame();
    }

    /// Runs `task` with the stream of the round trip on the renderer thread, and returns its result.
    fn on_stream<T, F>(&self, task: F) -> Result<T, NativeMethodError>
    where
        T: Send + 'static,
        F: FnOnce(&mut Stream<SurfaceStream>) -> Result<T, NativeMethodError> + Send + 'static,
    {
        let stream = self.stream;
        run_on_streams(&self.run.host, move |streams, _| {
            task((streams.stream(stream)).map_err(NativeMethodError::FindStream)?)
        })
    }
}

/// Runs `task` with the streams of the run of `host` and returns its result.
fn run_on_streams<T, F>(host: &AospHost, task: F) -> Result<T, NativeMethodError>
where
    T: Send + 'static,
    F: FnOnce(&mut AospStreams, &Painter) -> Result<T, NativeMethodError> + Send + 'static,
{
    (host.run_on_streams(task)).map_err(NativeMethodError::RunEnded)?
}

/// Creates the stream that notifies `notify` of its frames, and attaches it to the renderer.
fn attach_stream(
    streams: &mut AospStreams,
    sizes: BufferSizes,
    format: StreamFormat,
    notify: mpsc::Sender<()>,
) -> Result<StreamId, lavashell_android::Error> {
    let mut stream = SurfaceStream::new(sizes, format)?;
    stream.on_frame(move || match notify.send(()) {
        // Notifications sent after the round trip is dropped are discarded.
        Ok(()) | Err(mpsc::SendError(())) => {}
    });
    Ok(streams.attach(stream))
}

/// Latches the newest frame of `stream`. Returns the latch and a copy of the fence of the frame that the producer has
/// not finished writing.
fn latch_once(
    streams: &mut AospStreams,
    painter: &Painter,
    stream: StreamId,
) -> Result<(Latch, Option<OwnedFd>), NativeMethodError> {
    let latch = (streams.latch(painter, stream)).map_err(NativeMethodError::Latch)?;
    let attached = (streams.stream(stream)).map_err(NativeMethodError::FindStream)?;
    let fence = (attached.source().producer_fence())
        .map(|fence| fence.try_clone_to_owned())
        .transpose()
        .map_err(NativeMethodError::CopyProducerFence)?;
    Ok((latch, fence))
}

/// A hardware buffer passed to the thread of the renderer.
struct SentBuffer(HardwareBufferRef);

impl SentBuffer {
    fn get(&self) -> &HardwareBuffer {
        &self.0
    }
}

// SAFETY: AOSP counts the references of an AHardwareBuffer atomically, so a reference may be released on any thread.
// The renderer only reads the properties of the buffer, which AOSP and Vulkan allow from any thread. The ndk type is
// not `Send` only because it contains a raw pointer.
unsafe impl Send for SentBuffer {}

/// What a round trip reads next: a frame presented to the target `ImageReader`, or the end of the run.
enum TargetEvent {
    Presented,
    RunEnded(Result<(), NativeMethodError>),
}

/// The thread of the run that draws the pane into the target `ImageReader`, and the host of that run.
struct Run {
    host: AospHost,
    thread: Option<JoinHandle<()>>,
}

impl Run {
    fn start(
        size: Size,
        requested: Receiver<Pane>,
        ended: mpsc::Sender<TargetEvent>,
    ) -> Result<Self, NativeMethodError> {
        let host = AospHost::new();
        let served = host.clone();
        let thread = (thread::Builder::new().name("round trip".to_owned()))
            .spawn(move || {
                let outcome = served.serve(|| run_pane(size, requested));
                let _round_trip_left = ended.send(TargetEvent::RunEnded(outcome));
            })
            .map_err(NativeMethodError::SpawnRun)?;
        Ok(Self {
            host,
            thread: Some(thread),
        })
    }
}

impl Drop for Run {
    /// Destroys the Surface, which closes the window and ends the run, and joins the run thread.
    fn drop(&mut self) {
        self.host.surface_destroyed();
        if let Some(thread) = self.thread.take() {
            let _panic_fails_the_test_without_a_result = thread.join();
        }
    }
}

/// The `ImageReader` into which the run draws and from which the round trip reads the drawn colors back.
struct TargetReader {
    reader: ImageReader,
    events: mpsc::Receiver<TargetEvent>,
}

impl TargetReader {
    /// A reader of `width` × `height` pixels, and the sender through which the run reports its end to the reader.
    fn new(
        width: i32,
        height: i32,
    ) -> Result<(Self, mpsc::Sender<TargetEvent>), NativeMethodError> {
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
        let (ended, events) = mpsc::channel();
        let presented = ended.clone();
        reader
            .set_image_listener(Box::new(move |_| {
                match presented.send(TargetEvent::Presented) {
                    Ok(()) | Err(mpsc::SendError(_)) => {}
                }
            }))
            .map_err(NativeMethodError::CreateTarget)?;
        Ok((Self { reader, events }, ended))
    }

    /// Waits for the next frame presented to the reader and passes that frame to `read`, which returns before the
    /// frame is released.
    ///
    /// # Errors
    /// Fails when the run ends before a frame is presented, or as `read` does.
    fn read_next_frame<T>(
        &mut self,
        read: impl FnOnce(&Image) -> Result<T, NativeMethodError>,
    ) -> Result<T, NativeMethodError> {
        match (self.events.recv()).map_err(NativeMethodError::AwaitNotification)? {
            TargetEvent::Presented => {}
            TargetEvent::RunEnded(outcome) => {
                outcome?;
                return Err(NativeMethodError::RunEndedEarly);
            }
        }
        match (self.reader.acquire_next_image()).map_err(NativeMethodError::ReadTarget)? {
            AcquireResult::Image(frame) => read(&frame),
            AcquireResult::NoBufferAvailable | AcquireResult::MaxImagesAcquired => {
                Err(NativeMethodError::NoTargetFrame)
            }
        }
    }

    /// Deletes the reader once the run has ended, and returns the outcome of the run.
    fn finish(self) -> Result<(), NativeMethodError> {
        let Self { reader, events } = self;
        drop(reader);
        // The reader and the run thread are the only senders. Once both are dropped, the events end.
        for event in events {
            if let TargetEvent::RunEnded(outcome) = event {
                return outcome;
            }
        }
        Ok(())
    }
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
