// SPDX-License-Identifier: GPL-3.0-only
use crate::error::NativeMethodError;
use crate::pane::{Pane, run_pane};
use lavashell_actor::{Receiver, Sender, mailbox};
use lavashell_aosp::{
    AospHost, AospStreams, BufferSizes, HardwareBufferMemory, HostConfiguration, HostDisplay,
    MotionEvent, NativeWindow, StreamFormat, SurfaceStream,
};
use lavashell_core::prelude::*;
use lavashell_renderer::{Latch, Painter, Stream, skia};
use lavashell_vulkan_renderer::{ForeignAlpha, ForeignFormat, ForeignImporter, ForeignMemory};
use ndk::hardware_buffer::{HardwareBuffer, HardwareBufferRef, HardwareBufferUsage};
use ndk::media::image_reader::{AcquireResult, Image, ImageFormat, ImageReader};
use rustix::event::{PollFd, PollFlags, poll};
use rustix::io::Errno;
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::sync::mpsc;
use std::thread::{self, JoinHandle};

pub struct StreamRoundTrip {
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
    pub fn new(
        sizes: BufferSizes,
        format: StreamFormat,
        display: HostDisplay,
        configuration: HostConfiguration,
    ) -> Result<Self, NativeMethodError> {
        let (width, height) = sizes.default_lengths();
        let (mut target, ended) = TargetReader::new(width, height)?;
        let window = (target.reader.window()).map_err(NativeMethodError::CreateTarget)?;
        let (requested, panes) = mailbox();
        let size = Size::from_px(width.px(), height.px(), display.density);
        let run = Run::start(AospHost::new(display, configuration), size, panes, ended)?;
        run.host.surface_created(NativeWindow::new(window));
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

    pub fn producer_window(&self) -> Result<ndk::native_window::NativeWindow, NativeMethodError> {
        self.on_stream(|stream| Ok(stream.source().native_window().clone()))
    }

    pub fn resize(&self, sizes: BufferSizes) -> Result<(), NativeMethodError> {
        self.on_stream(move |stream| {
            (stream.source_mut().resize(sizes)).map_err(NativeMethodError::Resize)
        })
    }

    pub fn read_buffer_format(
        &self,
        buffer: HardwareBufferRef,
    ) -> Result<ForeignFormat, NativeMethodError> {
        let buffer = SentBuffer(buffer);
        run_on_streams(&self.run.host, move |streams, _| {
            read_format(streams.foreign_importer(), buffer.get())
        })
    }

    pub fn drawn_size(&self) -> Result<Option<PixelSize>, NativeMethodError> {
        self.on_stream(|stream| Ok(stream.drawn_size()))
    }

    pub fn report_motion(&self, event: MotionEvent) {
        self.run.host.motion(event);
    }

    pub fn wakes(&self) -> u32 {
        self.wakes
    }

    pub fn await_wake(&mut self) -> Result<(), NativeMethodError> {
        (self.arrived.recv()).map_err(NativeMethodError::AwaitNotification)?;
        self.wakes = self.wakes.saturating_add(1);
        Ok(())
    }

    pub fn close(self) -> Result<u32, NativeMethodError> {
        let Self {
            run,
            target,
            arrived,
            ..
        } = self;
        drop(run);
        target.finish()?;
        Ok(arrived.iter().fold(0, |count, ()| count.saturating_add(1)))
    }

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

    pub fn render(&mut self) -> Result<skia::Color, NativeMethodError> {
        let image = self.image.ok_or(NativeMethodError::NothingLatched)?;
        self.show(Pane::Whole(image));
        let center = find_center(self.pane);
        self.target
            .read_next_frame(|frame| read_pixel(frame, center))
    }

    pub fn render_grouped(&mut self, opacity: f64) -> Result<[skia::Color; 2], NativeMethodError> {
        let image = self.image.ok_or(NativeMethodError::NothingLatched)?;
        let pane = self.pane;
        let middle = find_center(pane).x;
        let left = skia::IRect::new(pane.left, pane.top, middle, pane.bottom);
        let right = skia::IRect::new(middle, pane.top, pane.right, pane.bottom);
        self.show(Pane::Grouped {
            image,
            opacity,
            shift: left.width().px(),
        });
        let (left, right) = (find_center(left), find_center(right));
        self.target
            .read_next_frame(|frame| Ok([read_pixel(frame, left)?, read_pixel(frame, right)?]))
    }

    fn show(&self, pane: Pane) {
        let _ended_run_fails_the_next_read = self.requested.push(pane);
        self.run.host.frame();
    }

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

fn run_on_streams<T, F>(host: &AospHost, task: F) -> Result<T, NativeMethodError>
where
    T: Send + 'static,
    F: FnOnce(&mut AospStreams, &Painter) -> Result<T, NativeMethodError> + Send + 'static,
{
    (host.run_on_streams(task)).map_err(NativeMethodError::RunEnded)?
}

fn read_format(
    importer: &ForeignImporter,
    buffer: &HardwareBuffer,
) -> Result<ForeignFormat, NativeMethodError> {
    let description = buffer.describe();
    let size = PixelSize::new(description.width, description.height);
    let memory = HardwareBufferMemory::read(importer, buffer, size, ForeignAlpha::Premultiplied)
        .map_err(NativeMethodError::BufferFormat)?
        .ok_or(NativeMethodError::UnimportableBuffer)?;
    Ok(memory.description().format)
}

fn attach_stream(
    streams: &mut AospStreams,
    sizes: BufferSizes,
    format: StreamFormat,
    notify: mpsc::Sender<()>,
) -> Result<StreamId, lavashell_aosp::Error> {
    let mut stream = SurfaceStream::new(sizes, format)?;
    stream.on_frame(move || {
        let _closed_round_trip_counts_no_wakes = notify.send(());
    });
    Ok(streams.attach(stream))
}

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

struct SentBuffer(HardwareBufferRef);

impl SentBuffer {
    fn get(&self) -> &HardwareBuffer {
        &self.0
    }
}

// SAFETY: The buffer has to cross to the thread of the streams, because the Vulkan device that reads its format lives
// only there and `AospHost::run_on_streams` runs its task there. Reading the format needs the AHardwareBuffer itself,
// so no copy of its description can take its place. The safe form was tried first. The task closure does not compile
// without this wrapper, because `HardwareBufferRef` is not `Send`. The ndk type lacks `Send` only because it contains
// a raw pointer. AOSP counts the references of an AHardwareBuffer atomically, so a reference may be released on any
// thread. The task only reads the properties of the buffer, which AOSP and Vulkan allow from any thread.
unsafe impl Send for SentBuffer {}

enum TargetEvent {
    Presented,
    RunEnded(Result<(), NativeMethodError>),
}

struct Run {
    host: AospHost,
    thread: Option<JoinHandle<()>>,
}

fn serve_pane(
    host: AospHost,
    size: Size,
    requested: Receiver<Pane>,
) -> Result<(), NativeMethodError> {
    host.serve(|link| run_pane(size, requested, link))
        .map_err(NativeMethodError::Serve)?
}

impl Run {
    fn start(
        host: AospHost,
        size: Size,
        requested: Receiver<Pane>,
        ended: mpsc::Sender<TargetEvent>,
    ) -> Result<Self, NativeMethodError> {
        let served = host.clone();
        let thread = (thread::Builder::new().name("round trip".to_owned()))
            .spawn(move || {
                let outcome = serve_pane(served, size, requested);
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
    fn drop(&mut self) {
        self.host.surface_destroyed();
        if let Some(thread) = self.thread.take() {
            let _panic_fails_the_test_without_a_result = thread.join();
        }
    }
}

struct TargetReader {
    reader: ImageReader,
    events: mpsc::Receiver<TargetEvent>,
}

impl TargetReader {
    fn new(
        width: i32,
        height: i32,
    ) -> Result<(Self, mpsc::Sender<TargetEvent>), NativeMethodError> {
        let usage = HardwareBufferUsage::CPU_READ_OFTEN | HardwareBufferUsage::GPU_COLOR_OUTPUT;
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
                let _closed_target_awaits_no_frames = presented.send(TargetEvent::Presented);
            }))
            .map_err(NativeMethodError::CreateTarget)?;
        Ok((Self { reader, events }, ended))
    }

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

    fn finish(self) -> Result<(), NativeMethodError> {
        let Self { reader, events } = self;
        drop(reader);
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
