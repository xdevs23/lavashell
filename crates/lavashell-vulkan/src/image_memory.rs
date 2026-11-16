// SPDX-License-Identifier: GPL-3.0-only
use crate::{Device, Error, Image, ImageFormat};
use ash::vk;
use std::ffi::CStr;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Allocation {
    pub size: vk::DeviceSize,
    pub memory_type: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sharing {
    Private,
    Export(vk::ExternalMemoryHandleTypeFlags),
    Import(vk::ExternalMemoryHandleTypeFlags),
}

impl Sharing {
    pub(crate) fn handle_type(self) -> Option<vk::ExternalMemoryHandleTypeFlags> {
        match self {
            Self::Private => None,
            Self::Export(handle_type) | Self::Import(handle_type) => Some(handle_type),
        }
    }

    pub(crate) fn required_features(self) -> vk::ExternalMemoryFeatureFlags {
        match self {
            Self::Private => vk::ExternalMemoryFeatureFlags::empty(),
            Self::Export(_) => vk::ExternalMemoryFeatureFlags::EXPORTABLE,
            Self::Import(_) => vk::ExternalMemoryFeatureFlags::IMPORTABLE,
        }
    }
}

/// # Safety
/// Where `sharing` returns [`Sharing::Import`], the structure that `memory_chain` chains must import one live
/// allocation of that handle type, and `allocation` must return the size of that allocation and a memory type that
/// the driver reports for it. Where the image is created with [`ImageFormat::External`], `image_chain` must chain the
/// external format that the driver reports for the allocation. Every chained structure must live through the call that
/// receives the chain.
pub unsafe trait ImageMemory {
    fn sharing(&self) -> Sharing;

    fn required_extensions(&self) -> &'static [&'static CStr];

    fn allocation(
        &self,
        device: &Device,
        requirements: &vk::MemoryRequirements,
    ) -> Result<Allocation, Error>;

    fn memory_chain(
        &self,
        info: vk::MemoryAllocateInfo<'_>,
        allocate: impl FnOnce(&vk::MemoryAllocateInfo<'_>) -> Result<vk::DeviceMemory, Error>,
    ) -> Result<vk::DeviceMemory, Error>;

    fn image_chain(
        &self,
        info: vk::ImageCreateInfo<'_>,
        create: impl FnOnce(&vk::ImageCreateInfo<'_>) -> Result<vk::Image, Error>,
    ) -> Result<vk::Image, Error> {
        create(&info)
    }
}

impl Image {
    pub fn import_memory(
        device: &Arc<Device>,
        format: ImageFormat,
        extent: vk::Extent2D,
        usage: vk::ImageUsageFlags,
        memory: &impl ImageMemory,
    ) -> Result<Arc<Self>, Error> {
        Self::create(device, format, extent, usage, memory).map(Arc::new)
    }
}
