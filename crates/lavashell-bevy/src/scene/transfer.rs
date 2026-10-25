// SPDX-License-Identifier: GPL-3.0-only
//! Transfers of a scene image between the queue of Bevy and the consumer of the image: the first clear, the release
//! to the consumer and the acquire back from the consumer.

use crate::Error;
use ash::vk;
use lavashell_renderer::vulkan::PendingRelease;
use lavashell_vulkan::{
    Commands, ExportableImage, Image, ImageState, QueueFamily, Recorded, Recording, Status,
    Submitted,
};
use std::any::Any;
use std::sync::Arc;

/// The layout and the queue family of a released scene image. The producer releases the image in this state, and the
/// consumer returns the image in this state.
const RELEASED: ImageState = ImageState::new(vk::ImageLayout::GENERAL, QueueFamily::External);

/// The state that wgpu tracks for a scene image once a render is copied out of it.
const COPIED_OUT: ImageState =
    ImageState::new(vk::ImageLayout::TRANSFER_SRC_OPTIMAL, QueueFamily::Device);

/// Releases `image` to the consumer after a render was copied out of it. The release is recorded into `commands`.
pub(super) fn release(
    image: &ExportableImage,
    commands: Commands,
    lease: Arc<dyn Any + Send + Sync>,
) -> Result<PendingRelease, Error> {
    let mut recording = commands.record().map_err(Error::ReleaseScene)?;
    // SAFETY: Only wgpu knows the state that it tracks for an image. The recording API of lavashell-vulkan cannot read
    // that state, so `assume` accepts the state as a promise of the caller. `capture` moves the image into `COPIED_OUT`
    // through wgpu, in the submission just before these commands on the same queue. That submission also makes the
    // writes of the render available.
    unsafe { recording.assume(image.image(), COPIED_OUT) }.map_err(Error::ReleaseScene)?;
    PendingRelease::submit(recording, image, lease).map_err(Error::SubmitRelease)
}

/// One set of commands that changes the state of a scene image, recorded again for each submission.
pub(super) struct Transfer {
    image: Arc<Image>,
    state: TransferState,
}

/// The commands of a transfer, ready to record or on the queue.
enum TransferState {
    Idle(Commands),
    Submitted(Submitted),
}

impl Transfer {
    pub fn new(image: Arc<Image>, commands: Commands) -> Self {
        Self {
            image,
            state: TransferState::Idle(commands),
        }
    }

    pub fn status(&self) -> Result<Status, Error> {
        match &self.state {
            TransferState::Idle(_) => Ok(Status::Complete),
            TransferState::Submitted(submitted) => {
                submitted.status().map_err(Error::TransferStatus)
            }
        }
    }

    /// Clears a new image. The image remains owned by the queue of Bevy.
    pub fn initialize(self) -> Result<Self, Error> {
        self.submit(Error::InitializeScene, |recording, image| {
            recording.discard(image, vk::ImageLayout::TRANSFER_DST_OPTIMAL)?;
            recording.clear(image, vk::ClearColorValue::default())
        })
    }

    /// Acquires the image back from the consumer and moves it into `COPIED_OUT`, the state that wgpu tracks for it.
    pub fn acquire(self) -> Result<Self, Error> {
        self.submit(Error::AcquireScene, |recording, image| {
            // SAFETY: The state of an image returned by the consumer is a promise of the consumer. lavashell-vulkan
            // cannot check that promise, because its recording API cannot read the state of an image. The ring selects
            // a released target again only after the consumer has dropped its last lease. The consumer drops that lease
            // only after its own release of the image in `RELEASED` is complete.
            unsafe { recording.assume(image, RELEASED) }?;
            recording.transition(image, COPIED_OUT)
        })
    }

    /// Records and submits the commands again once their last submission is complete. Fails with
    /// `Error::PendingTransfer` while that submission is still pending.
    fn submit(
        self,
        failed: fn(lavashell_vulkan::Error) -> Error,
        record: impl FnOnce(&mut Recording, &Arc<Image>) -> Result<(), lavashell_vulkan::Error>,
    ) -> Result<Self, Error> {
        let commands = match self.state {
            TransferState::Idle(commands) => commands,
            TransferState::Submitted(submitted) => {
                if submitted.status().map_err(Error::TransferStatus)? == Status::Pending {
                    return Err(Error::PendingTransfer);
                }
                submitted.reclaim().map_err(Error::TransferStatus)?
            }
        };
        let mut recording = commands.record().map_err(failed)?;
        record(&mut recording, &self.image).map_err(failed)?;
        let submitted = recording
            .finish()
            .and_then(Recorded::submit)
            .map_err(failed)?;
        Ok(Self {
            image: self.image,
            state: TransferState::Submitted(submitted),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{SCENE_FORMAT, SCENE_USAGE};
    use crate::test_error::TestError;
    use crate::texture_types::image_description;
    use lavashell_vulkan::{Device, DeviceRequest, Features, Instance, InstanceRequest};

    #[test]
    fn a_completed_transfer_records_again() -> Result<(), TestError> {
        let instance = Instance::create(&InstanceRequest {
            api_version: vk::API_VERSION_1_1,
            extensions: &[],
        })?;
        let physical =
            (instance.physical_devices()?.first().copied()).ok_or(TestError::NoPhysicalDevice)?;
        let family = (physical.queue_families().iter())
            .position(|family| family.queue_flags.contains(vk::QueueFlags::GRAPHICS))
            .ok_or(TestError::NoGraphicsQueue)?;
        let device = Device::create(
            physical,
            &DeviceRequest {
                queue_family: u32::try_from(family)?,
                extensions: &[ash::khr::external_memory_fd::NAME],
                features: Features::default(),
            },
        )?;
        let extent = vk::Extent2D {
            width: 8,
            height: 8,
        };
        let description = image_description(SCENE_FORMAT, SCENE_USAGE, extent)?;
        let image = Image::exportable(&device, &description)?;
        let commands = Commands::new(&device)?;
        let transfer = Transfer::new(image.image().clone(), commands).initialize()?;
        device.wait_idle()?;
        assert_eq!(transfer.status()?, Status::Complete);
        let transfer = transfer.initialize()?;
        device.wait_idle()?;
        assert_eq!(transfer.status()?, Status::Complete);
        assert_eq!(image.export()?.description(), description);
        Ok(())
    }
}
