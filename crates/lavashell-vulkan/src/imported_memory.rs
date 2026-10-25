// SPDX-License-Identifier: GPL-3.0-only
use crate::image::Backing;
use crate::{Device, Error, Image, ImageFormat};
use ash::vk;
use std::ffi::CStr;
use std::sync::Arc;

/// Memory allocated outside this crate, such as a buffer that another process shares, which an image imports as its
/// memory. The platform that owns the memory implements this trait.
///
/// # Safety
/// The structure that `memory_chain` chains must import one live allocation of the handle type that `handle_type`
/// returns. `allocation` must return the size of that allocation and a memory type that the driver reports for it.
/// Where `format` returns [`ImageFormat::External`], `image_chain` must chain the external format that the driver
/// reports for the allocation. Every chained structure must live through the call that receives the chain.
pub unsafe trait ImportedMemory {
    /// The device extensions that the import needs.
    fn required_extensions(&self) -> &'static [&'static CStr];

    fn handle_type(&self) -> vk::ExternalMemoryHandleTypeFlags;

    /// The format of the image that imports the memory.
    fn format(&self) -> ImageFormat;

    /// The size and the memory type of the allocation for an image with `requirements`.
    ///
    /// # Errors
    /// Fails when no memory type of the allocation suits the image.
    fn allocation(
        &self,
        requirements: &vk::MemoryRequirements,
    ) -> Result<(vk::DeviceSize, u32), Error>;

    /// Calls `allocate` with `info`, after chaining to `info` the structure that imports the memory.
    ///
    /// # Errors
    /// Fails when `allocate` fails.
    fn memory_chain(
        &self,
        info: vk::MemoryAllocateInfo<'_>,
        allocate: impl FnOnce(&vk::MemoryAllocateInfo<'_>) -> Result<vk::DeviceMemory, Error>,
    ) -> Result<vk::DeviceMemory, Error>;

    /// Calls `create` with `info`, after chaining to `info` what the image needs beyond its format, size, usage and
    /// handle type, such as an external format.
    ///
    /// # Errors
    /// Fails when `create` fails.
    fn image_chain(
        &self,
        info: vk::ImageCreateInfo<'_>,
        create: impl FnOnce(&vk::ImageCreateInfo<'_>) -> Result<vk::Image, Error>,
    ) -> Result<vk::Image, Error> {
        create(&info)
    }
}

impl<M: ImportedMemory> Backing for M {
    fn required_extensions(&self) -> &'static [&'static CStr] {
        ImportedMemory::required_extensions(self)
    }

    fn handle_type(&self) -> Option<vk::ExternalMemoryHandleTypeFlags> {
        Some(ImportedMemory::handle_type(self))
    }

    fn required_features(&self) -> vk::ExternalMemoryFeatureFlags {
        vk::ExternalMemoryFeatureFlags::IMPORTABLE
    }

    fn allocation(
        &self,
        _device: &Device,
        requirements: &vk::MemoryRequirements,
    ) -> Result<(vk::DeviceSize, u32), Error> {
        ImportedMemory::allocation(self, requirements)
    }

    fn memory_chain(
        &self,
        info: vk::MemoryAllocateInfo<'_>,
        allocate: impl FnOnce(&vk::MemoryAllocateInfo<'_>) -> Result<vk::DeviceMemory, Error>,
    ) -> Result<vk::DeviceMemory, Error> {
        ImportedMemory::memory_chain(self, info, allocate)
    }

    fn image_chain(
        &self,
        info: vk::ImageCreateInfo<'_>,
        create: impl FnOnce(&vk::ImageCreateInfo<'_>) -> Result<vk::Image, Error>,
    ) -> Result<vk::Image, Error> {
        ImportedMemory::image_chain(self, info, create)
    }
}

impl Image {
    /// Creates an image of size `extent`, in the format of `memory`, whose memory is `memory`.
    ///
    /// # Errors
    /// Fails when the device lacks an extension of the import, cannot create the image or cannot import the memory.
    pub fn import_memory(
        device: &Arc<Device>,
        extent: vk::Extent2D,
        usage: vk::ImageUsageFlags,
        memory: &impl ImportedMemory,
    ) -> Result<Arc<Self>, Error> {
        Self::create(device, memory.format(), extent, usage, memory).map(Arc::new)
    }
}
