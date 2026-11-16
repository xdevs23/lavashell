// SPDX-License-Identifier: GPL-3.0-only
use crate::fd::exported_fd;
use crate::image_memory::Allocation;
use crate::{Error, Image, ImageDescription};
use ash::vk;
use std::os::fd::OwnedFd;
use std::sync::Arc;

#[derive(Clone)]
pub struct ExportableImage {
    image: Arc<Image>,
    description: ImageDescription,
}

impl ExportableImage {
    pub(crate) fn new(image: Image, description: ImageDescription) -> Self {
        Self {
            image: Arc::new(image),
            description,
        }
    }

    #[must_use]
    pub fn image(&self) -> &Arc<Image> {
        &self.image
    }

    #[must_use]
    pub fn description(&self) -> ImageDescription {
        self.description
    }

    pub fn export(&self) -> Result<OpaqueFdExport, Error> {
        let device = self.image.device();
        let functions =
            ash::khr::external_memory_fd::Device::new(device.instance().ash(), device.ash());
        let info = vk::MemoryGetFdInfoKHR::default()
            .memory(self.image.memory())
            .handle_type(vk::ExternalMemoryHandleTypeFlags::OPAQUE_FD);
        // SAFETY: ash 0.38 has no safe form of vkGetMemoryFdKHR. The device enables the extension, which
        // `Image::exportable` required, and the memory is live and was allocated exportable as an opaque fd.
        let raw = unsafe { functions.get_memory_fd(&info) }.map_err(Error::ExportMemory)?;
        // SAFETY: `raw` comes from the export above, and an opaque fd export hands its descriptor to the caller alone.
        let fd = unsafe { exported_fd(raw) }.ok_or(Error::MemoryWithoutDescriptor)?;
        Ok(OpaqueFdExport::new(
            fd,
            self.description,
            self.image.allocation(),
            device.physical_device().device_uuid()?,
        ))
    }
}

pub struct OpaqueFdExport {
    fd: OwnedFd,
    description: ImageDescription,
    allocation: Allocation,
    device_uuid: [u8; vk::UUID_SIZE],
}

impl OpaqueFdExport {
    pub(crate) fn new(
        fd: OwnedFd,
        description: ImageDescription,
        allocation: Allocation,
        device_uuid: [u8; vk::UUID_SIZE],
    ) -> Self {
        Self {
            fd,
            description,
            allocation,
            device_uuid,
        }
    }

    pub fn try_clone(&self) -> Result<Self, Error> {
        Ok(Self {
            fd: self.fd.try_clone().map_err(Error::DuplicateDescriptor)?,
            ..*self
        })
    }

    pub(crate) fn fd(&self) -> &OwnedFd {
        &self.fd
    }

    #[must_use]
    pub fn description(&self) -> ImageDescription {
        self.description
    }

    pub(crate) fn allocation(&self) -> Allocation {
        self.allocation
    }

    pub(crate) fn device_uuid(&self) -> [u8; vk::UUID_SIZE] {
        self.device_uuid
    }
}
