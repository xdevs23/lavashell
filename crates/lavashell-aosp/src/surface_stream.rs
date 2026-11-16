// SPDX-License-Identifier: GPL-3.0-only
use crate::fenced_image::{AcquiredImage, WriteProgress, acquire_next};
use crate::platform::ndk::hardware_buffer::{HardwareBufferRef, HardwareBufferUsage};
use crate::platform::ndk::media::image_reader::{AcquireResult, ImageFormat, ImageReader};
use crate::platform::ndk::native_window::NativeWindow;
use crate::{BufferSizes, Error};
use lavashell_actor::Latest;
use lavashell_core::prelude::PixelSize;
use lavashell_renderer::{
    BufferQueue, FrameSource, LatchAccess, ProducedFrame, StreamRejection, Taken, skia,
};
use std::os::fd::{BorrowedFd, OwnedFd};
use std::rc::{Rc, Weak};
use std::sync::{Arc, mpsc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamFormat {
    Private,
    Rgba8888,
}

const IMAGES_ACQUIRED_AT_ONCE: i32 = SHOWN + WAITING_FOR_PRODUCER + ACQUIRED_DURING_ONE_LATCH;
const SHOWN: i32 = 1;
const WAITING_FOR_PRODUCER: i32 = 1;
const ACQUIRED_DURING_ONE_LATCH: i32 = 2;

type Wake = Box<dyn Fn() + Send + Sync>;
type WakeSlot = Arc<Latest<Wake>>;

pub struct SurfaceStream {
    format: StreamFormat,
    wake: WakeSlot,
    unwritten: Option<QueuedFrame>,
    shown: Reader,
    newest: Option<Reader>,
    retiring: Vec<Reader>,
}

impl SurfaceStream {
    pub fn new(sizes: BufferSizes, format: StreamFormat) -> Result<Self, Error> {
        let wake = WakeSlot::default();
        let shown = Reader::new(sizes, format, &wake)?;
        Ok(Self {
            format,
            shown,
            wake,
            unwritten: None,
            newest: None,
            retiring: Vec::new(),
        })
    }

    pub fn on_frame(&mut self, wake: impl Fn() + Send + Sync + 'static) {
        self.wake.publish(Box::new(wake));
    }

    #[must_use]
    pub fn native_window(&self) -> &NativeWindow {
        &self.producing().window
    }

    #[cfg(test)]
    #[must_use]
    pub fn waits_for_producer(&self) -> bool {
        self.unwritten.is_some()
    }

    #[must_use]
    pub fn producer_fence(&self) -> Option<BorrowedFd<'_>> {
        self.unwritten
            .as_ref()
            .and_then(|frame| frame.image.acquire_fence())
    }

    pub fn resize(&mut self, sizes: BufferSizes) -> Result<(), Error> {
        let reader = Reader::new(sizes, self.format, &self.wake)?;
        if let Some(replaced) = self.newest.replace(reader) {
            if let Some(frame) = self
                .unwritten
                .take_if(|frame| frame.reader.is_from(&replaced))
            {
                frame.release(None);
            }
            self.retire(replaced);
        }
        Ok(())
    }

    pub fn release_window(&mut self, window: &NativeWindow) {
        self.retiring.retain(|reader| reader.window != *window);
        for reader in std::iter::once(&mut self.shown).chain(&mut self.newest) {
            if reader.window == *window {
                reader.released = true;
            }
        }
    }

    fn retire(&mut self, reader: Reader) {
        if !reader.released {
            self.retiring.push(reader);
        }
    }

    fn producing(&self) -> &Reader {
        self.newest.as_ref().unwrap_or(&self.shown)
    }
}

impl FrameSource for SurfaceStream {
    type Frame = QueuedFrame;
    type Error = Error;

    fn take_newest(&mut self, _: &LatchAccess) -> Result<Taken<QueuedFrame>, Error> {
        let taken = match &self.newest {
            Some(newest) => match newest.take()? {
                Taken::Nothing => self.shown.take()?,
                taken => taken,
            },
            None => self.shown.take()?,
        };
        match taken {
            Taken::Frame(frame) => {
                if let Some(replaced) = self.unwritten.replace(frame) {
                    replaced.release(None);
                }
            }
            Taken::Rejected(rejection) => return Ok(Taken::Rejected(rejection)),
            Taken::Nothing => {}
        }
        let Some(frame) = self.unwritten.take() else {
            return Ok(Taken::Nothing);
        };
        match frame.image.write_progress()? {
            WriteProgress::Finished => Ok(Taken::Frame(frame)),
            WriteProgress::Ongoing => {
                self.unwritten = Some(frame);
                Ok(Taken::Nothing)
            }
        }
    }

    fn record_latch(&mut self, _: &LatchAccess, frame: &QueuedFrame) {
        if let Some(newest) = self.newest.take_if(|newest| frame.reader.is_from(newest)) {
            let replaced = std::mem::replace(&mut self.shown, newest);
            self.retire(replaced);
        }
    }

    fn take_removed_buffers(
        &mut self,
        _: &LatchAccess,
    ) -> impl Iterator<Item = Result<BufferId, Error>> {
        std::iter::once(&self.shown)
            .chain(&self.newest)
            .chain(&self.retiring)
            .flat_map(|reader| reader.removed.try_iter())
            .map(|removal| removal.map_err(Error::RemovedBufferId))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct BufferId(u64);

struct Reader {
    images: ImageReader,
    window: NativeWindow,
    max: PixelSize,
    removed: mpsc::Receiver<std::io::Result<BufferId>>,
    alive_marker: Rc<()>,
    released: bool,
}

#[derive(Clone)]
pub struct ReaderToken(Weak<()>);

impl ReaderToken {
    fn is_from(&self, reader: &Reader) -> bool {
        Weak::ptr_eq(&self.0, &Rc::downgrade(&reader.alive_marker))
    }
}

impl BufferQueue for ReaderToken {
    fn exists(&self) -> bool {
        self.0.strong_count() > 0
    }
}

impl Reader {
    fn new(sizes: BufferSizes, format: StreamFormat, wake: &WakeSlot) -> Result<Self, Error> {
        let (width, height) = sizes.default_lengths();
        let format = match format {
            StreamFormat::Private => ImageFormat::PRIVATE,
            StreamFormat::Rgba8888 => ImageFormat::RGBA_8888,
        };
        let usage = HardwareBufferUsage::GPU_SAMPLED_IMAGE;
        let mut reader =
            ImageReader::new_with_usage(width, height, format, usage, IMAGES_ACQUIRED_AT_ONCE)
                .map_err(Error::CreateReader)?;
        let (removals, removed) = mpsc::channel();
        reader
            .set_buffer_removed_listener(Box::new(move |_, buffer| {
                let _stream_dropped_so_nobody_reads_removals =
                    removals.send(buffer.id().map(BufferId));
            }))
            .map_err(Error::ListenForRemovals)?;
        let wake = wake.clone();
        reader
            .set_image_listener(Box::new(move |_| {
                if let Some(wake) = wake.read() {
                    wake();
                }
            }))
            .map_err(Error::ListenForFrames)?;
        let window = reader.window().map_err(Error::ReaderWindow)?;
        Ok(Self {
            images: reader,
            window,
            max: sizes.max(),
            removed,
            alive_marker: Rc::new(()),
            released: false,
        })
    }

    fn acquire_newest(&self) -> Result<Option<AcquiredImage>, Error> {
        let mut newest = None;
        loop {
            let acquired = match acquire_next(&self.images) {
                Ok(AcquireResult::Image(acquired)) => acquired,
                Ok(AcquireResult::NoBufferAvailable | AcquireResult::MaxImagesAcquired) => {
                    return Ok(newest);
                }
                Err(error) => {
                    if let Some(older) = newest {
                        older.return_after(None);
                    }
                    return Err(Error::AcquireFrame(error));
                }
            };
            if let Some(older) = newest.replace(acquired) {
                older.return_after(None);
            }
        }
    }

    fn take(&self) -> Result<Taken<QueuedFrame>, Error> {
        let Some(acquired) = self.acquire_newest()? else {
            return Ok(Taken::Nothing);
        };
        let buffer = acquired.hardware_buffer().map_err(Error::FrameBuffer)?;
        let description = buffer.describe();
        let size = PixelSize::new(description.width, description.height);
        let bounds = match size.signed() {
            Some((width, height))
                if size.width <= self.max.width && size.height <= self.max.height =>
            {
                skia::IRect::from_wh(width, height)
            }
            _ => {
                acquired.return_after(None);
                return Ok(Taken::Rejected(StreamRejection::TooLarge));
            }
        };
        let crop = acquired.crop_rect().map_err(Error::FrameCrop)?;
        let crop = skia::IRect::new(crop.left, crop.top, crop.right, crop.bottom);
        let Some(crop) = skia::IRect::intersect(&crop, &bounds) else {
            acquired.return_after(None);
            return Ok(Taken::Rejected(StreamRejection::CropOutside));
        };
        let length = |pixels: i32| u32::try_from(pixels).map_err(Error::CropLength);
        let cropped = PixelSize::new(length(crop.width())?, length(crop.height())?);
        Ok(Taken::Frame(QueuedFrame {
            id: BufferId(buffer.id().map_err(Error::BufferId)?),
            buffer: buffer.acquire(),
            origin: skia::IPoint::new(crop.left, crop.top),
            size: cropped,
            reader: ReaderToken(Rc::downgrade(&self.alive_marker)),
            image: acquired,
        }))
    }
}

pub struct QueuedFrame {
    image: AcquiredImage,
    pub(crate) buffer: HardwareBufferRef,
    id: BufferId,
    origin: skia::IPoint,
    size: PixelSize,
    reader: ReaderToken,
}

impl ProducedFrame for QueuedFrame {
    type BufferKey = BufferId;
    type Queue = ReaderToken;

    fn buffer_key(&self) -> BufferId {
        self.id
    }

    fn queue(&self) -> &ReaderToken {
        &self.reader
    }

    fn origin(&self) -> skia::IPoint {
        self.origin
    }

    fn size(&self) -> PixelSize {
        self.size
    }

    fn release(self, after: Option<OwnedFd>) {
        if self.reader.exists() {
            self.image.return_after(after);
        }
    }
}
