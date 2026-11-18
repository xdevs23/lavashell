// SPDX-License-Identifier: GPL-3.0-only

use crate::Error;
use ash::vk;
use lavashell_vulkan::{
    Commands, ExportableImage, Image, ImageState, QueueFamily, Recorded, Recording, Status,
    Submitted,
};
use lavashell_vulkan_renderer::vulkan::{PendingRelease, SceneLease};
use std::sync::Arc;

const RELEASED: ImageState = ImageState::new(vk::ImageLayout::GENERAL, QueueFamily::External);

const COPIED_OUT: ImageState =
    ImageState::new(vk::ImageLayout::TRANSFER_SRC_OPTIMAL, QueueFamily::Device);

pub(super) fn release(
    image: &ExportableImage,
    commands: Commands,
    lease: Arc<dyn SceneLease>,
) -> Result<PendingRelease, Error> {
    let mut recording = commands.record().map_err(Error::ReleaseScene)?;
    // SAFETY: Only wgpu knows the state that it tracks for an image. The recording API of lavashell-vulkan cannot read
    // that state, so `assume` accepts the state as a promise of the caller. `capture` moves the image into `COPIED_OUT`
    // through wgpu, in the submission just before these commands on the same queue. That submission also makes the
    // writes of the render available.
    unsafe { recording.assume(image.image(), COPIED_OUT) }.map_err(Error::ReleaseScene)?;
    PendingRelease::submit(recording, image, lease).map_err(Error::SubmitRelease)
}

pub(super) struct Transfer {
    image: Arc<Image>,
    state: TransferState,
}

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

    pub fn initialize(self) -> Result<Self, Error> {
        self.submit(Error::InitializeScene, |recording, image| {
            recording.discard(image, vk::ImageLayout::TRANSFER_DST_OPTIMAL)?;
            recording.clear(image, vk::ClearColorValue::default())
        })
    }

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
                submitted.reclaim().map_err(Error::ReclaimTransfer)?
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
