// SPDX-License-Identifier: GPL-3.0-only
use crate::fd::exported_fd;
use crate::{Error, Image, ImageDescription};
use ash::vk;
use std::os::fd::OwnedFd;
use std::sync::Arc;

/// An image in device-local memory that [`ExportableImage::export`] exports as an opaque file descriptor, for another
/// Vulkan device or process to import.
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

    /// Exports the memory of the image as a new opaque file descriptor, together with the values that an importer needs
    /// to bind that memory.
    ///
    /// # Errors
    /// Fails when the driver cannot export the memory.
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
            self.image.allocation_bytes(),
            self.image.memory_type(),
            device.physical_device().device_uuid()?,
        ))
    }
}

/// The opaque file descriptor of a dedicated memory allocation that contains one image. The image description,
/// allocation size, memory type and device UUID come with it, because an importer needs them to bind the memory to an
/// image that it creates.
pub struct OpaqueFdExport {
    fd: OwnedFd,
    description: ImageDescription,
    allocation_bytes: u64,
    memory_type: u32,
    device_uuid: [u8; vk::UUID_SIZE],
}

impl OpaqueFdExport {
    pub(crate) fn new(
        fd: OwnedFd,
        description: ImageDescription,
        allocation_bytes: u64,
        memory_type: u32,
        device_uuid: [u8; vk::UUID_SIZE],
    ) -> Self {
        Self {
            fd,
            description,
            allocation_bytes,
            memory_type,
            device_uuid,
        }
    }

    /// # Errors
    /// Fails when the process cannot duplicate the descriptor.
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

    #[must_use]
    pub fn allocation_bytes(&self) -> u64 {
        self.allocation_bytes
    }

    #[must_use]
    pub fn memory_type(&self) -> u32 {
        self.memory_type
    }

    #[must_use]
    pub fn device_uuid(&self) -> [u8; vk::UUID_SIZE] {
        self.device_uuid
    }
}
