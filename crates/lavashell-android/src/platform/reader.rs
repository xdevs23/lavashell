// SPDX-License-Identifier: GPL-3.0-only
//! A simulation of the image reader of the ndk crate, with a producer controlled by the tests.
//! The code of the stream, the latch and the render runs against it unchanged.

use super::image::{
    AcquireResult, HardwareBuffer, HardwareBufferFormat, HardwareBufferUsage, Image, ImageFormat,
};
use super::ndk::media_error::MediaError;
use super::{NativeState, NativeWindow, TestError};
use lavashell_core::prelude::PixelSize;
use lavashell_renderer::skia;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    io::{ErrorKind, Write},
    os::fd::{AsRawFd, IntoRawFd, OwnedFd, RawFd},
    os::unix::fs::MetadataExt,
    rc::Rc,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Event {
    /// The stream returned a frame to the producer, after a fence or right away.
    Released {
        id: u64,
        fenced: bool,
    },
    Imported(u64),
    /// Skia wrapped the import for the paints.
    Acquired(u64),
    /// A release returned the import to the producer.
    HandedBack(u64),
    ImportDropped(u64),
    ReaderDeleted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Writing {
    /// The producer finished writing before it queued the buffer, so the acquire fence has signalled.
    Finished,
    /// The producer finishes writing once the test calls `Producer::finish_writing`.
    Pending,
    /// The producer finished writing before it queued the buffer, and the buffer arrives without an acquire fence.
    Unfenced,
}

#[derive(Clone)]
pub(crate) struct Buffer {
    pub id: u64,
    pub size: PixelSize,
    pub layers: u32,
    pub usage: HardwareBufferUsage,
    /// The left, top, right and bottom edge of the crop set by the producer. `None` crops to the whole buffer.
    pub crop: Option<(i32, i32, i32, i32)>,
    pub format: HardwareBufferFormat,
    pub color: skia::Color,
    pub writing: Writing,
    pub undescribable: bool,
    pub fails_import: bool,
    /// Reading the id of the buffer fails.
    pub unidentifiable: bool,
}

impl Buffer {
    pub fn new(id: u64, size: PixelSize, color: skia::Color) -> Self {
        Self {
            id,
            size,
            layers: 1,
            usage: HardwareBufferUsage::GPU_SAMPLED_IMAGE,
            crop: None,
            format: HardwareBufferFormat::R8G8B8A8_UNORM,
            color,
            writing: Writing::Finished,
            undescribable: false,
            fails_import: false,
            unidentifiable: false,
        }
    }
}

// The listeners are called with the queue shared by the simulated reader and its producer. The listeners of the ndk
// crate are called with the reader instead.
type ImageListener = Box<dyn FnMut(&Queue) + Send>;
type RemovedListener = Box<dyn FnMut(&Queue, &HardwareBuffer) + Send>;

/// The buffer queue of one reader, shared with the producer controlled by the tests.
pub(crate) struct Queue {
    window: Rc<NativeState>,
    /// The width and the height of the default buffers, as the NDK reports them.
    pub(super) lengths: (i32, i32),
    queued: RefCell<Vec<Rc<(Buffer, Log)>>>,
    pub(super) acquired: Cell<i32>,
    max_images: i32,
    /// The write ends of the acquire fences of the buffers still being written, by buffer id.
    writing: RefCell<BTreeMap<u64, std::fs::File>>,
    pub(super) log: Log,
    /// The fences passed to the images of the reader, each with the inode that it referred to when
    /// it was passed. The reader closes these fences, as `AImage` does.
    kept: RefCell<Vec<(OwnedFd, std::io::Result<u64>)>>,
    pub(super) deleted: Cell<bool>,
    image_listener: RefCell<Option<ImageListener>>,
    removed_listener: RefCell<Option<RemovedListener>>,
}

pub(super) type Log = Rc<RefCell<Vec<Event>>>;

/// The simulated media service of the test running on this thread.
#[derive(Default)]
pub(crate) struct Media {
    /// Every reader fails to return its native window.
    pub fails_window: Cell<bool>,
}

thread_local! {
    pub(crate) static MEDIA: Media = Media::default();
}

/// The inode that `fd` refers to.
fn inode(fd: RawFd) -> std::io::Result<u64> {
    std::fs::metadata(format!("/proc/self/fd/{fd}")).map(|metadata| metadata.ino())
}

impl Queue {
    /// The default size of the buffers. `ImageReader::new_with_usage` has checked that both lengths are positive.
    pub(super) fn size(&self) -> PixelSize {
        PixelSize::new(self.lengths.0.unsigned_abs(), self.lengths.1.unsigned_abs())
    }

    /// Stores `fence`, passed to an image of the reader, so that the queue closes the fence when it is dropped.
    pub(super) fn keep(&self, fence: OwnedFd) {
        let number = fence.as_raw_fd();
        let mut kept = self.kept.borrow_mut();
        assert!(
            kept.iter().all(|(held, _)| held.as_raw_fd() != number),
            "the fence {number} was handed to the reader twice"
        );
        kept.push((fence, inode(number)));
    }
}

/// The producer of a Surface, writing through the native window of the Surface like a real producer.
pub(crate) struct Producer(Rc<Queue>);

impl Producer {
    pub fn of(window: &NativeWindow) -> Result<Self, TestError> {
        window
            .queue()
            .cloned()
            .map(Self)
            .ok_or(TestError::NotAReaderWindow)
    }

    pub fn queue(&self, buffer: Buffer) {
        assert!(
            !self.0.deleted.get(),
            "a producer queued into a deleted reader"
        );
        self.0
            .queued
            .borrow_mut()
            .push(Rc::new((buffer, self.0.log.clone())));
        if let Some(listener) = self.0.image_listener.borrow_mut().as_mut() {
            listener(&self.0);
        }
    }

    /// Frees `buffer` from the queue, like a producer that reallocates the buffers.
    pub fn remove(&self, buffer: Buffer) {
        let buffer = HardwareBuffer::new(Rc::new((buffer, self.0.log.clone())));
        if let Some(listener) = self.0.removed_listener.borrow_mut().as_mut() {
            listener(&self.0, &buffer);
        }
    }

    pub fn finish_writing(&self, id: u64) -> Result<(), TestError> {
        let mut fence =
            (self.0.writing.borrow_mut().remove(&id)).ok_or(TestError::NotBeingWritten(id))?;
        fence.write_all(&[1]).map_err(TestError::SignalFence)
    }

    pub fn default_size(&self) -> PixelSize {
        self.0.size()
    }

    pub fn events(&self) -> Vec<Event> {
        self.0.log.borrow().clone()
    }

    /// The numbers of the fences that were passed to the images of the reader and have since been closed or replaced
    /// by the stream. The reader removes each such number from its list without closing it, because the number no
    /// longer refers to that fence.
    pub fn broken_fences(&self) -> Result<Vec<RawFd>, TestError> {
        let mut kept = self.0.kept.borrow_mut();
        let mut intact = Vec::new();
        let mut broken = Vec::new();
        for (fence, inode_then) in std::mem::take(&mut *kept) {
            let inode_then = inode_then.map_err(TestError::FenceInode)?;
            let unchanged = match inode(fence.as_raw_fd()) {
                Ok(inode_now) => inode_now == inode_then,
                Err(error) if error.kind() == ErrorKind::NotFound => false,
                Err(error) => return Err(TestError::FenceInode(error)),
            };
            if unchanged {
                intact.push((fence, Ok(inode_then)));
            } else {
                broken.push(fence.into_raw_fd());
            }
        }
        *kept = intact;
        Ok(broken)
    }

    pub fn clear_events(&self) {
        self.0.log.borrow_mut().clear();
    }
}

pub struct ImageReader(Rc<Queue>);

impl ImageReader {
    pub fn new_with_usage(
        width: i32,
        height: i32,
        _: ImageFormat,
        _: HardwareBufferUsage,
        max_images: i32,
    ) -> Result<Self, MediaError> {
        // AImageReader_new fails for a width or a height that is not positive.
        if width <= 0 || height <= 0 {
            return Err(MediaError::InvalidParameter);
        }
        Ok(Self(Rc::new(Queue {
            window: Rc::default(),
            lengths: (width, height),
            queued: RefCell::default(),
            acquired: Cell::new(0),
            max_images,
            writing: RefCell::default(),
            log: Log::default(),
            kept: RefCell::default(),
            deleted: Cell::new(false),
            image_listener: RefCell::default(),
            removed_listener: RefCell::default(),
        })))
    }

    /// Fails when a listener was set before, because a frame may still be calling the earlier listener.
    pub fn set_image_listener(&mut self, listener: ImageListener) -> Result<(), MediaError> {
        let mut set = self.0.image_listener.borrow_mut();
        if set.is_some() {
            return Err(MediaError::InvalidOperation);
        }
        *set = Some(listener);
        Ok(())
    }

    /// Fails when a listener was set before, because a removal may still be calling the earlier listener.
    pub fn set_buffer_removed_listener(
        &mut self,
        listener: RemovedListener,
    ) -> Result<(), MediaError> {
        let mut set = self.0.removed_listener.borrow_mut();
        if set.is_some() {
            return Err(MediaError::InvalidOperation);
        }
        *set = Some(listener);
        Ok(())
    }

    pub fn window(&self) -> Result<NativeWindow, MediaError> {
        if MEDIA.with(|media| media.fails_window.get()) {
            return Err(MediaError::InvalidParameter);
        }
        Ok(NativeWindow::of_reader(&self.0.window, &self.0))
    }

    /// # Safety
    /// The fence returned with an image must signal before the image is read, as in the ndk crate.
    pub unsafe fn acquire_next_image_async(
        &self,
    ) -> Result<AcquireResult<(Image, Option<OwnedFd>)>, MediaError> {
        let queued_while_written = match self.0.queued.borrow().first() {
            None => return Ok(AcquireResult::NoBufferAvailable),
            Some(oldest) => {
                oldest.0.writing == Writing::Pending
                    && self.0.writing.borrow().contains_key(&oldest.0.id)
            }
        };
        if self.0.acquired.get() == self.0.max_images {
            return Ok(AcquireResult::MaxImagesAcquired);
        }
        // The producer queued the buffer again before the producer finished writing the buffer.
        if queued_while_written {
            return Err(MediaError::InvalidOperation);
        }
        let oldest = self.0.queued.borrow_mut().remove(0);
        self.0.acquired.set(self.0.acquired.get() + 1);
        let buffer = &oldest.0;
        let fence = match buffer.writing {
            Writing::Unfenced => None,
            Writing::Finished => Some(signalled_fence().map_err(MediaError::AcquireFence)?),
            Writing::Pending => {
                let (fence, signal) = pending_fence().map_err(MediaError::AcquireFence)?;
                self.0.writing.borrow_mut().extend([(buffer.id, signal)]);
                Some(fence)
            }
        };
        Ok(AcquireResult::Image((Image::new(oldest, &self.0), fence)))
    }
}

impl Drop for ImageReader {
    fn drop(&mut self) {
        self.0.deleted.set(true);
        *self.0.image_listener.borrow_mut() = None;
        *self.0.removed_listener.borrow_mut() = None;
        self.0.log.borrow_mut().push(Event::ReaderDeleted);
    }
}

/// A sync fence simulated by the read end of a pipe, which signals once a byte is written into the write end.
fn pending_fence() -> std::io::Result<(OwnedFd, std::fs::File)> {
    let (read, write) = rustix::pipe::pipe()?;
    Ok((read, std::fs::File::from(write)))
}

/// A simulated sync fence that has signalled.
pub(super) fn signalled_fence() -> std::io::Result<OwnedFd> {
    let (fence, mut signal) = pending_fence()?;
    signal.write_all(&[1])?;
    Ok(fence)
}
