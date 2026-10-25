// SPDX-License-Identifier: GPL-3.0-only
use crate::Error;
use crate::fenced_image::{AcquiredImage, WriteProgress, acquire_next};
use crate::platform::ndk::hardware_buffer::{HardwareBufferRef, HardwareBufferUsage};
use crate::platform::ndk::media::image_reader::{AcquireResult, ImageFormat, ImageReader};
use crate::platform::ndk::native_window::NativeWindow;
use lavashell_core::prelude::{PixelSize, StreamId, StreamImage};
use lavashell_renderer::{StreamRejection, skia};
use std::os::fd::{BorrowedFd, OwnedFd};
use std::rc::{Rc, Weak};
use std::sync::{Arc, Mutex, PoisonError, mpsc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamFormat {
    /// Any buffer format, chosen by the producer and the system. The documentation of `SurfaceStream` lists which
    /// formats are drawn.
    Private,
    Rgba8888,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BufferSizes {
    /// The size of the buffers when the producer does not choose another size. Must not exceed `max`.
    pub default: PixelSize,
    /// The size of the largest buffer drawn. A wider or taller buffer is released at the latch
    /// without being drawn.
    pub max: PixelSize,
}

/// How many images a stream acquires from one reader at once: the frame shown, the frame that waits until its producer
/// has written it, and the two images acquired at once during a latch. Those two are the newest image found so far and
/// the next one.
const IMAGES: i32 = SHOWN + WAITING_FOR_PRODUCER + LATCHING;
const SHOWN: i32 = 1;
const WAITING_FOR_PRODUCER: i32 = 1;
const LATCHING: i32 = 2;

type Wake = Arc<dyn Fn() + Send + Sync>;
type WakeSlot = Arc<Mutex<Option<Wake>>>;

/// The consumer end of a Surface drawn into by another process, such as an app drawing a window. Pass the
/// Surface of `native_window` to the producer. On the render thread, latch the newest frame with
/// `AndroidRenderer::latch` and draw the returned image with `Drawing::stream_image`.
///
/// The producer never waits for the host. While no latch is called, a newly queued frame replaces the frame
/// waiting in the reader. A hidden window needs no latch. The newest frame waits for the first latch after
/// the window shows again.
///
/// The buffers of a Surface replaced by `resize` remain until the host releases the Surface with `release_window`.
/// A producer that still draws into the old Surface after the resize loses no frame.
///
/// Buffers in RGBA or RGBX with 8 bits per channel are always drawn. On a Vulkan device that samples through a
/// YCbCr conversion, buffers in every other format are drawn too, such as YUV from a video decoder. They are
/// converted to RGB with the YCbCr model and range that the driver reports as suggested for each buffer. Without that
/// support, the latch releases a buffer in any other format and records a draw failure.
pub struct SurfaceStream {
    id: StreamId,
    format: StreamFormat,
    wake: WakeSlot,
    // Declared before the readers, so that the waiting frame is dropped before the reader that delivered it.
    unwritten: Option<QueuedFrame>,
    shown: Reader,
    newest: Option<Reader>,
    retiring: Vec<Reader>,
    latest: Option<StreamImage>,
}

impl SurfaceStream {
    /// # Errors
    /// Fails when the sizes do not fit the NDK, the default size exceeds the maximum, or AOSP cannot create the
    /// reader of the Surface.
    pub fn new(sizes: BufferSizes, format: StreamFormat) -> Result<Self, Error> {
        let wake = WakeSlot::default();
        let shown = Reader::new(sizes, format, &wake)?;
        Ok(Self {
            id: StreamId::next(),
            format,
            shown,
            wake,
            unwritten: None,
            newest: None,
            retiring: Vec::new(),
            latest: None,
        })
    }

    /// Sets `wake` in place of the previous callback. The reader runs `wake` on one of its threads when a frame arrives
    /// while no other frame waits. A frame that replaces a waiting frame does not run `wake`.
    pub fn on_frame(&mut self, wake: impl Fn() + Send + Sync + 'static) {
        let wake: Wake = Arc::new(wake);
        *self.wake.lock().unwrap_or_else(PoisonError::into_inner) = Some(wake);
    }

    /// The native window of the Surface for the producer, valid until the host releases the Surface after a
    /// `resize`, or until the drop of the stream.
    #[must_use]
    pub fn native_window(&self) -> &NativeWindow {
        &self.producing().window
    }

    /// Whether a frame waits in the stream until its producer has written it. The callback set by
    /// `on_frame` does not run again for that frame, so latch again on a later render.
    #[must_use]
    pub fn waits_for_producer(&self) -> bool {
        self.unwritten.is_some()
    }

    /// The fence that signals once the producer has written the frame waiting in the stream, `None` while no frame
    /// waits for its producer.
    #[must_use]
    pub fn producer_fence(&self) -> Option<BorrowedFd<'_>> {
        self.unwritten
            .as_ref()
            .and_then(|frame| frame.image.acquire_fence())
    }

    /// The size in pixels of the frame latched last, after the crop of the buffer.
    #[must_use]
    pub fn drawn_size(&self) -> Option<PixelSize> {
        self.latest.map(|image| image.size)
    }

    /// Creates a new Surface for the producer. The stream shows the frames of the previous Surface until the new
    /// Surface delivers a frame.
    ///
    /// # Errors
    /// Fails as `new` does.
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

    /// Lets the stream drop the buffers of `window`, a native window returned by `native_window` that the producer no
    /// longer draws into. The reader behind `window` is deleted as soon as the stream neither shows the frames of that
    /// Surface nor returns the Surface from `native_window`. A window of another stream is ignored.
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

    pub(crate) fn id(&self) -> StreamId {
        self.id
    }

    /// The ids of the buffers freed by the producers since the last call. An item is an error when the id of a freed
    /// buffer cannot be read.
    pub(crate) fn removed(&self) -> impl Iterator<Item = Result<BufferId, Error>> {
        std::iter::once(&self.shown)
            .chain(&self.newest)
            .chain(&self.retiring)
            .flat_map(|reader| reader.removed.try_iter())
            .map(|removal| removal.map_err(Error::RemovedBufferId))
    }

    /// Acquires the newest frame queued on the newest Surface, or on the shown Surface while the newest Surface has
    /// none. Returns the frame once the producer has finished writing it. Until then, the frame waits in the stream
    /// and the result is `Acquired::Nothing`. The frames queued before that frame return to the producer behind their
    /// acquire fences.
    pub(crate) fn take(&mut self) -> Result<Acquired, Error> {
        let acquired = match &self.newest {
            Some(newest) => match newest.take()? {
                Acquired::Nothing => self.shown.take()?,
                acquired => acquired,
            },
            None => self.shown.take()?,
        };
        match acquired {
            Acquired::Frame(frame) => {
                if let Some(replaced) = self.unwritten.replace(frame) {
                    replaced.release(None);
                }
            }
            Acquired::Rejected(rejection) => return Ok(Acquired::Rejected(rejection)),
            Acquired::Nothing => {}
        }
        let Some(frame) = self.unwritten.take() else {
            return Ok(Acquired::Nothing);
        };
        match frame.image.write_progress()? {
            WriteProgress::Finished => Ok(Acquired::Frame(frame)),
            WriteProgress::Ongoing => {
                self.unwritten = Some(frame);
                Ok(Acquired::Nothing)
            }
        }
    }

    /// Records `frame` as latched and returns the image that draws it. A frame of the newest Surface makes that
    /// Surface the shown one, and the Surface shown before is retired.
    pub(crate) fn record_latch(&mut self, frame: &QueuedFrame) -> StreamImage {
        if let Some(newest) = self.newest.take_if(|newest| frame.reader.is_from(newest)) {
            let replaced = std::mem::replace(&mut self.shown, newest);
            self.retire(replaced);
        }
        let generation = self.latest.map_or(1, |image| image.generation + 1);
        let image = StreamImage {
            stream: self.id,
            generation,
            size: frame.size,
        };
        self.latest = Some(image);
        image
    }
}

/// The id of a hardware buffer, unique in the system.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct BufferId(u64);

struct Reader {
    images: ImageReader,
    window: NativeWindow,
    /// The size of the largest buffer drawn. Both lengths fit into the signed lengths of the NDK and Skia.
    max: PixelSize,
    removed: mpsc::Receiver<std::io::Result<BufferId>>,
    life: Rc<()>,
    released: bool,
}

#[derive(Clone)]
pub(crate) struct ReaderToken(Weak<()>);

impl ReaderToken {
    pub fn is_alive(&self) -> bool {
        self.0.strong_count() > 0
    }

    fn is_from(&self, reader: &Reader) -> bool {
        Weak::ptr_eq(&self.0, &Rc::downgrade(&reader.life))
    }
}

impl Reader {
    fn new(sizes: BufferSizes, format: StreamFormat, wake: &WakeSlot) -> Result<Self, Error> {
        let (Some((width, height)), Some(_)) = (sizes.default.signed(), sizes.max.signed()) else {
            return Err(Error::SizesBeyondNdk(sizes));
        };
        if sizes.default.width > sizes.max.width || sizes.default.height > sizes.max.height {
            return Err(Error::DefaultBeyondMax(sizes));
        }
        let format = match format {
            StreamFormat::Private => ImageFormat::PRIVATE,
            StreamFormat::Rgba8888 => ImageFormat::RGBA_8888,
        };
        let usage = HardwareBufferUsage::GPU_SAMPLED_IMAGE;
        let mut reader = ImageReader::new_with_usage(width, height, format, usage, IMAGES)
            .map_err(Error::CreateReader)?;
        let (removals, removed) = mpsc::channel();
        reader
            .set_buffer_removed_listener(Box::new(move |_, buffer| {
                match removals.send(buffer.id().map(BufferId)) {
                    // A stream that dropped the receiver has also dropped every import of its buffers, so the
                    // removal can be ignored.
                    Ok(()) | Err(mpsc::SendError(_)) => {}
                }
            }))
            .map_err(Error::ListenForRemovals)?;
        // `on_frame` changes the shared slot, not the listener, because replacing the listener frees the earlier
        // listener while a frame may still be calling it.
        let wake = wake.clone();
        reader
            .set_image_listener(Box::new(move |_| {
                let current = wake.lock().unwrap_or_else(PoisonError::into_inner).clone();
                if let Some(wake) = current {
                    wake();
                }
            }))
            .map_err(Error::ListenForFrames)?;
        let window = reader.window().map_err(Error::ReaderWindow)?;
        Ok(Self {
            images: reader,
            window,
            max: sizes.max,
            removed,
            life: Rc::new(()),
            released: false,
        })
    }

    /// Acquires every queued image and returns the newest one. Each older image returns to the producer behind its
    /// acquire fence. While the GPU reads the frames held for it, every image of the reader can be in use. The newest
    /// frame then waits in the reader for a later latch.
    fn newest(&self) -> Result<Option<AcquiredImage>, Error> {
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

    fn take(&self) -> Result<Acquired, Error> {
        let Some(acquired) = self.newest()? else {
            return Ok(Acquired::Nothing);
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
                return Ok(Acquired::Rejected(StreamRejection::TooLarge));
            }
        };
        let crop = acquired.crop_rect().map_err(Error::FrameCrop)?;
        let crop = skia::IRect::new(crop.left, crop.top, crop.right, crop.bottom);
        let Some(crop) = skia::IRect::intersect(&crop, &bounds) else {
            acquired.return_after(None);
            return Ok(Acquired::Rejected(StreamRejection::CropOutside));
        };
        let length = |pixels: i32| u32::try_from(pixels).map_err(Error::CropLength);
        let cropped = PixelSize::new(length(crop.width())?, length(crop.height())?);
        Ok(Acquired::Frame(QueuedFrame {
            id: BufferId(buffer.id().map_err(Error::BufferId)?),
            buffer: buffer.acquire(),
            origin: skia::IPoint::new(crop.left, crop.top),
            size: cropped,
            reader: ReaderToken(Rc::downgrade(&self.life)),
            image: acquired,
        }))
    }
}

pub(crate) enum Acquired {
    /// The producer has not queued a frame since the last latch, or the producer is still writing the queued frame.
    Nothing,
    Frame(QueuedFrame),
    /// The reader returned the newest frame to the producer without drawing it.
    Rejected(StreamRejection),
}

/// A frame acquired from a reader.
pub(crate) struct QueuedFrame {
    image: AcquiredImage,
    pub buffer: HardwareBufferRef,
    pub id: BufferId,
    /// The top left corner of the crop of the buffer. `size` is the size of that crop.
    pub origin: skia::IPoint,
    pub size: PixelSize,
    pub reader: ReaderToken,
}

impl QueuedFrame {
    pub(crate) fn release(self, after: Option<OwnedFd>) {
        // A deleted reader has already closed the images acquired from it, and a fence passed to such an
        // image would never be closed. In that case, dropping `self` closes `after` and the acquire fence.
        if self.reader.is_alive() {
            self.image.return_after(after);
        }
    }
}
