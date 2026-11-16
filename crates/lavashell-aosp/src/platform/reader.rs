// SPDX-License-Identifier: GPL-3.0-only

use super::image::{
    AcquireResult, HardwareBuffer, HardwareBufferFormat, HardwareBufferUsage, Image, ImageFormat,
};
use super::ndk::media_error::MediaError;
use super::{NativeWindow, TestError};
use lavashell_core::prelude::PixelSize;
use lavashell_renderer::skia;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    io::{ErrorKind, Write},
    os::fd::{AsRawFd, IntoRawFd, OwnedFd, RawFd},
    os::unix::fs::MetadataExt,
    rc::{Rc, Weak},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Event {
    Released { id: u64, fenced: bool },
    Imported(u64),
    Acquired(u64),
    HandedBack(u64),
    ImportDropped(u64),
    ReaderDeleted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Writing {
    Finished,
    Pending,
    Unfenced,
}

#[derive(Clone)]
pub(crate) struct Buffer {
    pub id: u64,
    pub size: PixelSize,
    pub layers: u32,
    pub usage: HardwareBufferUsage,
    pub crop: Option<(i32, i32, i32, i32)>,
    pub format: HardwareBufferFormat,
    pub color: skia::Color,
    pub writing: Writing,
    pub undescribable: bool,
    pub fails_import: bool,
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

type ImageListener = Box<dyn FnMut(&Queue) + Send>;
type RemovedListener = Box<dyn FnMut(&Queue, &HardwareBuffer) + Send>;

pub(crate) struct Queue {
    pub(super) key: usize,
    pub(super) size: PixelSize,
    queued: RefCell<Vec<Rc<(Buffer, Log)>>>,
    pub(super) acquired: Cell<i32>,
    max_images: i32,
    writing: RefCell<BTreeMap<u64, std::fs::File>>,
    pub(super) log: Log,
    kept: RefCell<Vec<(OwnedFd, std::io::Result<u64>)>>,
    pub(super) deleted: Cell<bool>,
    image_listener: RefCell<Option<ImageListener>>,
    removed_listener: RefCell<Option<RemovedListener>>,
}

pub(super) type Log = Rc<RefCell<Vec<Event>>>;

#[derive(Default)]
pub(crate) struct Media {
    pub fails_window: Cell<bool>,
}

thread_local! {
    pub(super) static MEDIA: RefCell<Rc<Media>> = RefCell::default();
    pub(super) static QUEUES: RefCell<Vec<Weak<Queue>>> = const { RefCell::new(Vec::new()) };
}

fn inode(fd: RawFd) -> std::io::Result<u64> {
    std::fs::metadata(format!("/proc/self/fd/{fd}")).map(|metadata| metadata.ino())
}

impl Queue {
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

pub(crate) struct Producer(Rc<Queue>);

impl Producer {
    pub fn of(window: &NativeWindow) -> Result<Self, TestError> {
        let key = window.reader().ok_or(TestError::NotAReaderWindow)?;
        let queue = QUEUES.with_borrow(|queues| queues.get(key).and_then(Weak::upgrade));
        queue.map(Self).ok_or(TestError::NotAReaderWindow)
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
        self.0.size
    }

    pub fn events(&self) -> Vec<Event> {
        self.0.log.borrow().clone()
    }

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
        let size = match (u32::try_from(width), u32::try_from(height)) {
            (Ok(width @ 1..), Ok(height @ 1..)) => PixelSize::new(width, height),
            _ => return Err(MediaError::InvalidParameter),
        };
        let queue = Rc::new(Queue {
            key: QUEUES.with_borrow(Vec::len),
            size,
            queued: RefCell::default(),
            acquired: Cell::new(0),
            max_images,
            writing: RefCell::default(),
            log: Log::default(),
            kept: RefCell::default(),
            deleted: Cell::new(false),
            image_listener: RefCell::default(),
            removed_listener: RefCell::default(),
        });
        QUEUES.with_borrow_mut(|queues| queues.push(Rc::downgrade(&queue)));
        Ok(Self(queue))
    }

    pub fn set_image_listener(&mut self, listener: ImageListener) -> Result<(), MediaError> {
        let mut set = self.0.image_listener.borrow_mut();
        if set.is_some() {
            return Err(MediaError::InvalidOperation);
        }
        *set = Some(listener);
        Ok(())
    }

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
        if MEDIA.with_borrow(|media| media.fails_window.get()) {
            return Err(MediaError::InvalidParameter);
        }
        Ok(NativeWindow::of_reader(&self.0))
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

fn pending_fence() -> std::io::Result<(OwnedFd, std::fs::File)> {
    let (read, write) = rustix::pipe::pipe()?;
    Ok((read, std::fs::File::from(write)))
}

pub(super) fn signalled_fence() -> std::io::Result<OwnedFd> {
    let (fence, mut signal) = pending_fence()?;
    signal.write_all(&[1])?;
    Ok(fence)
}
