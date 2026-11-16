// SPDX-License-Identifier: GPL-3.0-only

use crate::Error;
use crate::platform::ndk::hardware_buffer::HardwareBuffer;
use crate::platform::ndk::media::image_reader::{AcquireResult, CropRect, Image, ImageReader};
use crate::platform::ndk::media_error::MediaError;
use rustix::event::{PollFd, PollFlags, Timespec, poll};
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WriteProgress {
    Finished,
    Ongoing,
}

pub(crate) struct AcquiredImage {
    image: Image,
    acquire: Option<OwnedFd>,
}

pub(crate) fn acquire_next(
    reader: &ImageReader,
) -> Result<AcquireResult<AcquiredImage>, MediaError> {
    // SAFETY: A latch must never wait for a producer. Only the asynchronous acquire of the NDK returns the acquire
    // fence without waiting for it, and ndk 0.9.0 offers that acquire only as an unsafe fn. Its condition is that the
    // pixels of the image are accessed only after the fence has signalled. `AcquiredImage` stores the fence with the
    // image and returns only the buffer handle and the crop. The stream passes a frame to the import of its pixels only
    // after `write_progress` has returned `Finished`.
    let acquired = unsafe { reader.acquire_next_image_async() }?;
    Ok(match acquired {
        AcquireResult::Image((image, acquire)) => {
            AcquireResult::Image(AcquiredImage { image, acquire })
        }
        AcquireResult::NoBufferAvailable => AcquireResult::NoBufferAvailable,
        AcquireResult::MaxImagesAcquired => AcquireResult::MaxImagesAcquired,
    })
}

impl AcquiredImage {
    pub fn hardware_buffer(&self) -> Result<HardwareBuffer, MediaError> {
        self.image.hardware_buffer()
    }

    pub fn crop_rect(&self) -> Result<CropRect, MediaError> {
        self.image.crop_rect()
    }

    pub fn acquire_fence(&self) -> Option<BorrowedFd<'_>> {
        self.acquire.as_ref().map(AsFd::as_fd)
    }

    pub fn write_progress(&self) -> Result<WriteProgress, Error> {
        let Some(acquire) = &self.acquire else {
            return Ok(WriteProgress::Finished);
        };
        let mut fences = [PollFd::new(acquire, PollFlags::IN)];
        let without_waiting = Timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        let ready = poll(&mut fences, Some(&without_waiting)).map_err(Error::PollAcquireFence)?;
        Ok(if ready > 0 {
            WriteProgress::Finished
        } else {
            WriteProgress::Ongoing
        })
    }

    pub fn return_after(self, after: Option<OwnedFd>) {
        match after.or(self.acquire) {
            Some(fence) => self.image.delete_async(fence),
            None => drop(self.image),
        }
    }
}
