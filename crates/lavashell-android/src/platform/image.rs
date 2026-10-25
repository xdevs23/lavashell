// SPDX-License-Identifier: GPL-3.0-only
//! Simulations of the image and hardware buffer types of the ndk crate, backed by the buffers of a simulated reader.

use super::reader::{Buffer, Event, Log, Queue};
use std::cell::Cell;
use std::os::fd::OwnedFd;
use std::rc::Rc;

/// The status of a failed call of the NDK media API, or the failure of the host to simulate the call.
#[derive(Debug, thiserror::Error)]
pub enum MediaError {
    #[error("an invalid parameter")]
    InvalidParameter,
    #[error("an invalid object")]
    InvalidObject,
    #[error("an invalid operation")]
    InvalidOperation,
    #[error("the host cannot make the acquire fence of a simulated image")]
    AcquireFence(#[source] std::io::Error),
}

/// An `AHARDWAREBUFFER_FORMAT` value of the NDK.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HardwareBufferFormat(u32);

impl HardwareBufferFormat {
    pub const R8G8B8A8_UNORM: Self = Self(1);
    pub const R8G8B8X8_UNORM: Self = Self(2);
    /// Sampled only through a YCbCr conversion, which the simulated device does not support.
    pub const R16G16B16A16_FLOAT: Self = Self(0x16);
}

/// An `AIMAGE_FORMAT` value of the NDK.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageFormat(u32);

impl ImageFormat {
    pub const PRIVATE: Self = Self(0x22);
    pub const RGBA_8888: Self = Self(1);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HardwareBufferUsage(u64);

impl HardwareBufferUsage {
    pub const GPU_SAMPLED_IMAGE: Self = Self(1 << 8);
    pub const PROTECTED_CONTENT: Self = Self(1 << 14);
    pub const GPU_MIPMAP_COMPLETE: Self = Self(1 << 26);
    pub const CPU_READ_OFTEN: Self = Self(3);

    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

impl std::ops::BitOr for HardwareBufferUsage {
    type Output = Self;

    fn bitor(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

pub enum AcquireResult<T> {
    Image(T),
    NoBufferAvailable,
    MaxImagesAcquired,
}

pub struct CropRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

pub struct Image {
    buffer: Rc<(Buffer, Log)>,
    queue: Rc<Queue>,
    released: Cell<bool>,
}

impl Image {
    pub fn hardware_buffer(&self) -> Result<HardwareBuffer, MediaError> {
        self.readable()?;
        Ok(HardwareBuffer(self.buffer.clone()))
    }

    pub fn crop_rect(&self) -> Result<CropRect, MediaError> {
        self.readable()?;
        let buffer = &self.buffer.0;
        // The NDK reports the lengths of a buffer as signed 32-bit integers.
        let (width, height) = buffer.size.signed().ok_or(MediaError::InvalidObject)?;
        let (left, top, right, bottom) = buffer.crop.unwrap_or((0, 0, width, height));
        Ok(CropRect {
            left,
            top,
            right,
            bottom,
        })
    }

    /// Fails once the reader of the image is deleted, as `AImage` does.
    fn readable(&self) -> Result<(), MediaError> {
        if self.queue.deleted.get() {
            return Err(MediaError::InvalidObject);
        }
        Ok(())
    }

    pub(super) fn new(buffer: Rc<(Buffer, Log)>, queue: &Rc<Queue>) -> Self {
        Self {
            buffer,
            queue: queue.clone(),
            released: Cell::new(false),
        }
    }

    pub fn delete_async(self, fence: OwnedFd) {
        assert!(
            !self.queue.deleted.get(),
            "a fence handed to an image of a deleted reader is never closed"
        );
        self.queue.keep(fence);
        self.release(true);
    }

    fn release(&self, fenced: bool) {
        self.released.set(true);
        self.queue.acquired.set(self.queue.acquired.get() - 1);
        let id = self.buffer.0.id;
        self.queue
            .log
            .borrow_mut()
            .push(Event::Released { id, fenced });
    }
}

impl Drop for Image {
    fn drop(&mut self) {
        if !self.released.get() {
            self.release(false);
        }
    }
}

pub struct HardwareBufferDesc {
    pub width: u32,
    pub height: u32,
}

pub struct HardwareBuffer(Rc<(Buffer, Log)>);

/// The ndk crate has a separate type for a buffer with an acquired reference. The simulated buffer is already
/// reference-counted, so one type is enough for both.
pub type HardwareBufferRef = HardwareBuffer;

impl HardwareBuffer {
    pub fn id(&self) -> std::io::Result<u64> {
        let buffer = &self.0.0;
        if buffer.unidentifiable {
            return Err(std::io::ErrorKind::InvalidInput.into());
        }
        Ok(buffer.id)
    }

    pub(super) fn new(buffer: Rc<(Buffer, Log)>) -> Self {
        Self(buffer)
    }

    pub fn describe(&self) -> HardwareBufferDesc {
        let size = self.0.0.size;
        HardwareBufferDesc {
            width: size.width,
            height: size.height,
        }
    }

    pub fn acquire(&self) -> HardwareBufferRef {
        Self(self.0.clone())
    }

    pub(super) fn contents(&self) -> (&Buffer, &Log) {
        (&self.0.0, &self.0.1)
    }
}
