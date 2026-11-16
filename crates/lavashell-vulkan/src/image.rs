// SPDX-License-Identifier: GPL-3.0-only
use crate::export::ExportableImage;
use crate::image_memory::{Allocation, ImageMemory, Sharing};
use crate::{Device, Error, ExtensionSet, OpaqueFdExport};
use ash::vk;
use std::ffi::CStr;
use std::os::fd::{AsRawFd, IntoRawFd};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageDescription {
    pub format: vk::Format,
    pub extent: vk::Extent2D,
    pub usage: vk::ImageUsageFlags,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageFormat {
    Known(vk::Format),
    External,
}

struct DeviceLocal;

struct Exported;

pub struct Image {
    handle: ImageHandle,
    memory: Memory,
}

struct ImageHandle {
    device: Arc<Device>,
    raw: vk::Image,
}

struct Memory {
    device: Arc<Device>,
    handle: vk::DeviceMemory,
    allocation: Allocation,
}

impl Image {
    pub fn new(device: &Arc<Device>, description: &ImageDescription) -> Result<Arc<Self>, Error> {
        Self::create_described(device, description, &DeviceLocal).map(Arc::new)
    }

    pub fn exportable(
        device: &Arc<Device>,
        description: &ImageDescription,
    ) -> Result<ExportableImage, Error> {
        let image = Self::create_described(device, description, &Exported)?;
        Ok(ExportableImage::new(image, *description))
    }

    pub fn import(device: &Arc<Device>, export: &OpaqueFdExport) -> Result<Arc<Self>, Error> {
        if export.device_uuid() != device.physical_device().device_uuid()? {
            return Err(Error::DifferentDevice);
        }
        Self::create_described(device, &export.description(), export).map(Arc::new)
    }

    fn create_described(
        device: &Arc<Device>,
        description: &ImageDescription,
        backing: &impl ImageMemory,
    ) -> Result<Self, Error> {
        let format = ImageFormat::Known(description.format);
        Self::create(
            device,
            format,
            description.extent,
            description.usage,
            backing,
        )
    }

    pub(crate) fn create(
        device: &Arc<Device>,
        format: ImageFormat,
        extent: vk::Extent2D,
        usage: vk::ImageUsageFlags,
        backing: &impl ImageMemory,
    ) -> Result<Self, Error> {
        device.require(backing.required_extensions())?;
        let sharing = backing.sharing();
        let mut info = image_info(extent, usage);
        match format {
            ImageFormat::Known(format) => {
                check_support(device, format, extent, usage, backing)?;
                info = info.format(format);
            }
            ImageFormat::External => info = info.format(vk::Format::UNDEFINED),
        }
        let mut external_memory = vk::ExternalMemoryImageCreateInfo::default();
        if let Some(handle_type) = sharing.handle_type() {
            external_memory = external_memory.handle_types(handle_type);
            info = info.push_next(&mut external_memory);
        }
        let handle = backing.image_chain(info, |info| {
            // SAFETY: ash 0.38 has no safe form of vkCreateImage. The device is live through its `Arc`. For a known
            // format, `check_support` found the format, usage, extent and handle type of `info` supported. An
            // external format is chained by imported memory, and the contract of `ImageMemory` requires one that
            // the driver reported for that memory. The chain of `info` lives through the call.
            unsafe { device.ash().create_image(info, None) }.map_err(Error::CreateImage)
        })?;
        let image = ImageHandle {
            device: device.clone(),
            raw: handle,
        };
        let memory = Memory::allocate(&image, backing)?;
        // SAFETY: ash 0.38 has no safe form of vkBindImageMemory. The image and the memory belong to the live
        // device. The image has no memory bound yet. The memory is a dedicated allocation for the image that fits the
        // memory requirements of the image.
        unsafe { device.ash().bind_image_memory(handle, memory.handle, 0) }
            .map_err(Error::BindMemory)?;
        Ok(Self {
            handle: image,
            memory,
        })
    }

    #[must_use]
    pub fn handle(&self) -> vk::Image {
        self.handle.raw
    }

    #[must_use]
    pub fn device(&self) -> &Arc<Device> {
        &self.handle.device
    }

    pub(crate) fn allocation(&self) -> Allocation {
        self.memory.allocation
    }

    pub(crate) fn memory(&self) -> vk::DeviceMemory {
        self.memory.handle
    }

    pub(crate) fn subresources() -> vk::ImageSubresourceRange {
        vk::ImageSubresourceRange::default()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .level_count(1)
            .layer_count(1)
    }
}

impl Drop for ImageHandle {
    fn drop(&mut self) {
        // SAFETY: ash 0.38 has no safe form of vkDestroyImage. The image belongs to the live device. Commands that
        // use the image have an `Arc` of the `Image` until their submission completes.
        unsafe { self.device.ash().destroy_image(self.raw, None) };
    }
}

impl Memory {
    fn allocate(image: &ImageHandle, backing: &impl ImageMemory) -> Result<Self, Error> {
        let device = &image.device;
        // SAFETY: ash 0.38 has no safe form of vkGetImageMemoryRequirements. The image belongs to the live device.
        let requirements = unsafe { device.ash().get_image_memory_requirements(image.raw) };
        let allocation = backing.allocation(device, &requirements)?;
        let mut dedicated = vk::MemoryDedicatedAllocateInfo::default().image(image.raw);
        let info = vk::MemoryAllocateInfo::default()
            .allocation_size(allocation.size)
            .memory_type_index(allocation.memory_type)
            .push_next(&mut dedicated);
        let handle = backing.memory_chain(info, |info| {
            // SAFETY: ash 0.38 has no safe form of vkAllocateMemory. The allocation is dedicated to the live image
            // and fits the memory requirements of the image. An imported file descriptor contains an allocation of
            // this size and type, as recorded in its `OpaqueFdExport`. Other imported memory has the size and the
            // memory type that its `ImageMemory::allocation` returned, which the contract of that trait binds to
            // the allocation.
            unsafe { device.ash().allocate_memory(info, None) }.map_err(Error::AllocateMemory)
        })?;
        Ok(Self {
            device: device.clone(),
            handle,
            allocation,
        })
    }
}

impl Drop for Memory {
    fn drop(&mut self) {
        // SAFETY: ash 0.38 has no safe form of vkFreeMemory. The memory belongs to the live device. The image bound to
        // the memory is destroyed first, because `Image` declares `handle` before `memory` and Rust drops fields in
        // that order. Nothing else uses the memory.
        unsafe { self.device.ash().free_memory(self.handle, None) };
    }
}

// SAFETY: Nothing is imported or chained. The memory type is one that `requirements` allows.
unsafe impl ImageMemory for DeviceLocal {
    fn sharing(&self) -> Sharing {
        Sharing::Private
    }

    fn required_extensions(&self) -> &'static [&'static CStr] {
        &[]
    }

    fn allocation(
        &self,
        device: &Device,
        requirements: &vk::MemoryRequirements,
    ) -> Result<Allocation, Error> {
        Ok(Allocation {
            size: requirements.size,
            memory_type: device_local(device, requirements)?,
        })
    }

    fn memory_chain(
        &self,
        info: vk::MemoryAllocateInfo<'_>,
        allocate: impl FnOnce(&vk::MemoryAllocateInfo<'_>) -> Result<vk::DeviceMemory, Error>,
    ) -> Result<vk::DeviceMemory, Error> {
        allocate(&info)
    }
}

// SAFETY: The memory chain only asks the driver to make the allocation exportable. Nothing is imported.
unsafe impl ImageMemory for Exported {
    fn sharing(&self) -> Sharing {
        Sharing::Export(vk::ExternalMemoryHandleTypeFlags::OPAQUE_FD)
    }

    fn required_extensions(&self) -> &'static [&'static CStr] {
        &[ash::khr::external_memory_fd::NAME]
    }

    fn allocation(
        &self,
        device: &Device,
        requirements: &vk::MemoryRequirements,
    ) -> Result<Allocation, Error> {
        DeviceLocal.allocation(device, requirements)
    }

    fn memory_chain(
        &self,
        info: vk::MemoryAllocateInfo<'_>,
        allocate: impl FnOnce(&vk::MemoryAllocateInfo<'_>) -> Result<vk::DeviceMemory, Error>,
    ) -> Result<vk::DeviceMemory, Error> {
        let mut export = vk::ExportMemoryAllocateInfo::default()
            .handle_types(vk::ExternalMemoryHandleTypeFlags::OPAQUE_FD);
        allocate(&info.push_next(&mut export))
    }
}

// SAFETY: The memory chain imports a duplicate of the descriptor that the driver exported as an opaque fd. `allocation`
// returns the size and memory type recorded at that export, after checking them against the requirements of the image.
unsafe impl ImageMemory for OpaqueFdExport {
    fn sharing(&self) -> Sharing {
        Sharing::Import(vk::ExternalMemoryHandleTypeFlags::OPAQUE_FD)
    }

    fn required_extensions(&self) -> &'static [&'static CStr] {
        Exported.required_extensions()
    }

    fn allocation(
        &self,
        _device: &Device,
        requirements: &vk::MemoryRequirements,
    ) -> Result<Allocation, Error> {
        let allocation = self.allocation();
        let type_allowed = (requirements.memory_type_bits)
            .checked_shr(allocation.memory_type)
            .is_some_and(|bits| bits & 1 == 1);
        if requirements.size <= allocation.size && type_allowed {
            Ok(allocation)
        } else {
            Err(Error::ExportedMemoryMismatch)
        }
    }

    fn memory_chain(
        &self,
        info: vk::MemoryAllocateInfo<'_>,
        allocate: impl FnOnce(&vk::MemoryAllocateInfo<'_>) -> Result<vk::DeviceMemory, Error>,
    ) -> Result<vk::DeviceMemory, Error> {
        let duplicate = self.fd().try_clone().map_err(Error::DuplicateDescriptor)?;
        let mut import = vk::ImportMemoryFdInfoKHR::default()
            .handle_type(vk::ExternalMemoryHandleTypeFlags::OPAQUE_FD)
            .fd(duplicate.as_raw_fd());
        let memory = allocate(&info.push_next(&mut import))?;
        let _owned_by_the_driver = duplicate.into_raw_fd();
        Ok(memory)
    }
}

fn image_info(extent: vk::Extent2D, usage: vk::ImageUsageFlags) -> vk::ImageCreateInfo<'static> {
    vk::ImageCreateInfo::default()
        .image_type(vk::ImageType::TYPE_2D)
        .extent(extent.into())
        .mip_levels(1)
        .array_layers(1)
        .samples(vk::SampleCountFlags::TYPE_1)
        .tiling(vk::ImageTiling::OPTIMAL)
        .usage(usage)
        .sharing_mode(vk::SharingMode::EXCLUSIVE)
        .initial_layout(vk::ImageLayout::UNDEFINED)
}

fn device_local(device: &Device, requirements: &vk::MemoryRequirements) -> Result<u32, Error> {
    let properties = device.physical_device().memory_properties();
    let types = properties.memory_types_as_slice();
    (0_u32..)
        .zip(types)
        .find(|(index, memory)| {
            requirements.memory_type_bits & (1 << index) != 0
                && memory
                    .property_flags
                    .contains(vk::MemoryPropertyFlags::DEVICE_LOCAL)
        })
        .map(|(index, _)| index)
        .ok_or(Error::NoMemoryType)
}

fn check_support(
    device: &Device,
    format: vk::Format,
    extent: vk::Extent2D,
    usage: vk::ImageUsageFlags,
    backing: &impl ImageMemory,
) -> Result<(), Error> {
    let physical = device.physical_device();
    physical.require_version(vk::API_VERSION_1_1)?;
    let mut external_info = vk::PhysicalDeviceExternalImageFormatInfo::default();
    let mut info = vk::PhysicalDeviceImageFormatInfo2::default()
        .format(format)
        .ty(vk::ImageType::TYPE_2D)
        .tiling(vk::ImageTiling::OPTIMAL)
        .usage(usage);
    let sharing = backing.sharing();
    if let Some(handle_type) = sharing.handle_type() {
        external_info = external_info.handle_type(handle_type);
        info = info.push_next(&mut external_info);
    }
    let mut external = vk::ExternalImageFormatProperties::default();
    let mut properties = vk::ImageFormatProperties2::default().push_next(&mut external);
    let instance = physical.instance().ash();
    // SAFETY: ash 0.38 has no safe form of vkGetPhysicalDeviceImageFormatProperties2. The physical device belongs
    // to the live instance. The version check above confirmed Vulkan 1.1, which the call needs. Both chains contain
    // Vulkan 1.1 structures that live through the call.
    unsafe {
        instance.get_physical_device_image_format_properties2(
            physical.handle(),
            &info,
            &mut properties,
        )
    }
    .map_err(Error::ImageSupport)?;
    let maximum = properties.image_format_properties.max_extent;
    if extent.width > maximum.width || extent.height > maximum.height {
        return Err(Error::ImageExtent);
    }
    let shared = external.external_memory_properties.external_memory_features;
    if !shared.contains(sharing.required_features()) {
        return Err(Error::DeviceCannotShare);
    }
    Ok(())
}
