// SPDX-License-Identifier: GPL-3.0-only
use crate::export::ExportableImage;
use crate::{Device, Error, ExtensionSet, OpaqueFdExport};
use ash::vk;
use std::ffi::CStr;
use std::os::fd::{AsRawFd, IntoRawFd};
use std::sync::Arc;

/// The format, size and usage of an image. An image made from a description is a single-sample 2D color image with one
/// mip level and one layer, in optimal tiling and exclusive sharing mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageDescription {
    pub format: vk::Format,
    pub extent: vk::Extent2D,
    pub usage: vk::ImageUsageFlags,
}

/// The format of an image: a Vulkan format, or an external format, which the imported memory of the image chains to
/// the image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageFormat {
    Known(vk::Format),
    External,
}

/// Where the memory of an image comes from.
pub(crate) trait Backing {
    /// The device extensions that an image in this backing needs.
    fn required_extensions(&self) -> &'static [&'static CStr];

    /// The external handle type for which the image and its memory are made, or `None` for memory that only this
    /// device uses.
    fn handle_type(&self) -> Option<vk::ExternalMemoryHandleTypeFlags>;

    /// The external memory features, such as export or import, that the device must support for the handle type of an
    /// image in this backing.
    fn required_features(&self) -> vk::ExternalMemoryFeatureFlags;

    /// The size and the memory type of the allocation for an image with `requirements`.
    fn allocation(
        &self,
        device: &Device,
        requirements: &vk::MemoryRequirements,
    ) -> Result<(vk::DeviceSize, u32), Error>;

    /// Calls `allocate` with `info`, after chaining to `info` the export or import structure needed by this backing.
    fn memory_chain(
        &self,
        info: vk::MemoryAllocateInfo<'_>,
        allocate: impl FnOnce(&vk::MemoryAllocateInfo<'_>) -> Result<vk::DeviceMemory, Error>,
    ) -> Result<vk::DeviceMemory, Error>;

    /// Calls `create` with `info`, after chaining to `info` what an image in this backing needs beyond its format,
    /// size, usage and handle type.
    fn image_chain(
        &self,
        info: vk::ImageCreateInfo<'_>,
        create: impl FnOnce(&vk::ImageCreateInfo<'_>) -> Result<vk::Image, Error>,
    ) -> Result<vk::Image, Error> {
        create(&info)
    }
}

/// Memory that only this device uses.
struct DeviceLocal;

/// Device-local memory allocated as exportable, so that another device or process can import it from an opaque file
/// descriptor.
struct Exported;

/// An image bound to a dedicated memory allocation. Dropping the `Image` destroys the image and frees the memory.
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
    bytes: u64,
    type_index: u32,
}

impl Image {
    /// Creates an image in device-local memory that only this device uses.
    ///
    /// # Errors
    /// Fails when the device cannot create the image or has no device-local memory for it.
    pub fn new(device: &Arc<Device>, description: &ImageDescription) -> Result<Arc<Self>, Error> {
        Self::described(device, description, &DeviceLocal).map(Arc::new)
    }

    /// Creates an image in device-local memory that [`ExportableImage::export`] can export as opaque file descriptors.
    ///
    /// # Errors
    /// Fails when the device lacks `VK_KHR_external_memory_fd`, cannot share the image, or has no device-local
    /// memory for it.
    pub fn exportable(
        device: &Arc<Device>,
        description: &ImageDescription,
    ) -> Result<ExportableImage, Error> {
        let image = Self::described(device, description, &Exported)?;
        Ok(ExportableImage::new(image, *description))
    }

    /// Creates the image described by `export` and binds it to the memory imported from the file descriptor in
    /// `export`.
    ///
    /// # Errors
    /// Fails when the device lacks `VK_KHR_external_memory_fd`, when the export belongs to another physical device
    /// or does not contain the image, or when the driver cannot import the memory.
    pub fn import(device: &Arc<Device>, export: &OpaqueFdExport) -> Result<Arc<Self>, Error> {
        if export.device_uuid() != device.physical_device().device_uuid()? {
            return Err(Error::DifferentDevice);
        }
        Self::described(device, &export.description(), export).map(Arc::new)
    }

    fn described(
        device: &Arc<Device>,
        description: &ImageDescription,
        backing: &impl Backing,
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
        backing: &impl Backing,
    ) -> Result<Self, Error> {
        device.require(backing.required_extensions())?;
        let mut info = image_info(extent, usage);
        match format {
            ImageFormat::Known(format) => {
                check_support(device, format, extent, usage, backing)?;
                info = info.format(format);
            }
            // Vulkan requires `VK_FORMAT_UNDEFINED` as the format of an image created with an external format.
            ImageFormat::External => info = info.format(vk::Format::UNDEFINED),
        }
        let mut external_memory = vk::ExternalMemoryImageCreateInfo::default();
        if let Some(handle_type) = backing.handle_type() {
            external_memory = external_memory.handle_types(handle_type);
            info = info.push_next(&mut external_memory);
        }
        let handle = backing.image_chain(info, |info| {
            // SAFETY: ash 0.38 has no safe form of vkCreateImage. The device is live through its `Arc`. For a known
            // format, `check_support` found the format, usage, extent and handle type of `info` supported. An
            // external format is chained by imported memory, and the contract of `ImportedMemory` requires one that
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

    #[must_use]
    pub fn allocation_bytes(&self) -> u64 {
        self.memory.bytes
    }

    #[must_use]
    pub fn memory_type(&self) -> u32 {
        self.memory.type_index
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
    fn allocate(image: &ImageHandle, backing: &impl Backing) -> Result<Self, Error> {
        let device = &image.device;
        // SAFETY: ash 0.38 has no safe form of vkGetImageMemoryRequirements. The image belongs to the live device.
        let requirements = unsafe { device.ash().get_image_memory_requirements(image.raw) };
        let (bytes, type_index) = backing.allocation(device, &requirements)?;
        let mut dedicated = vk::MemoryDedicatedAllocateInfo::default().image(image.raw);
        let info = vk::MemoryAllocateInfo::default()
            .allocation_size(bytes)
            .memory_type_index(type_index)
            .push_next(&mut dedicated);
        let handle = backing.memory_chain(info, |info| {
            // SAFETY: ash 0.38 has no safe form of vkAllocateMemory. The allocation is dedicated to the live image
            // and fits the memory requirements of the image. An imported file descriptor contains an allocation of
            // this size and type, as recorded in its `OpaqueFdExport`. Other imported memory has the size and the
            // memory type that its `ImportedMemory::allocation` returned, which the contract of that trait binds to
            // the allocation.
            unsafe { device.ash().allocate_memory(info, None) }.map_err(Error::AllocateMemory)
        })?;
        Ok(Self {
            device: device.clone(),
            handle,
            bytes,
            type_index,
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

impl Backing for DeviceLocal {
    fn required_extensions(&self) -> &'static [&'static CStr] {
        &[]
    }

    fn handle_type(&self) -> Option<vk::ExternalMemoryHandleTypeFlags> {
        None
    }

    fn required_features(&self) -> vk::ExternalMemoryFeatureFlags {
        vk::ExternalMemoryFeatureFlags::empty()
    }

    fn allocation(
        &self,
        device: &Device,
        requirements: &vk::MemoryRequirements,
    ) -> Result<(vk::DeviceSize, u32), Error> {
        Ok((requirements.size, device_local(device, requirements)?))
    }

    fn memory_chain(
        &self,
        info: vk::MemoryAllocateInfo<'_>,
        allocate: impl FnOnce(&vk::MemoryAllocateInfo<'_>) -> Result<vk::DeviceMemory, Error>,
    ) -> Result<vk::DeviceMemory, Error> {
        allocate(&info)
    }
}

impl Backing for Exported {
    fn required_extensions(&self) -> &'static [&'static CStr] {
        &[ash::khr::external_memory_fd::NAME]
    }

    fn handle_type(&self) -> Option<vk::ExternalMemoryHandleTypeFlags> {
        Some(vk::ExternalMemoryHandleTypeFlags::OPAQUE_FD)
    }

    fn required_features(&self) -> vk::ExternalMemoryFeatureFlags {
        vk::ExternalMemoryFeatureFlags::EXPORTABLE
    }

    fn allocation(
        &self,
        device: &Device,
        requirements: &vk::MemoryRequirements,
    ) -> Result<(vk::DeviceSize, u32), Error> {
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

/// Memory imported from an opaque file descriptor, exported by another logical device on the same physical device.
impl Backing for OpaqueFdExport {
    fn required_extensions(&self) -> &'static [&'static CStr] {
        Exported.required_extensions()
    }

    fn handle_type(&self) -> Option<vk::ExternalMemoryHandleTypeFlags> {
        Exported.handle_type()
    }

    fn required_features(&self) -> vk::ExternalMemoryFeatureFlags {
        vk::ExternalMemoryFeatureFlags::IMPORTABLE
    }

    fn allocation(
        &self,
        _device: &Device,
        requirements: &vk::MemoryRequirements,
    ) -> Result<(vk::DeviceSize, u32), Error> {
        let memory_type = self.memory_type();
        let fits = requirements.size <= self.allocation_bytes()
            && (requirements.memory_type_bits.checked_shr(memory_type))
                .is_some_and(|bits| bits & 1 == 1);
        if fits {
            Ok((self.allocation_bytes(), memory_type))
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

/// Returns the lowest index of a device-local memory type allowed by `requirements`.
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
    backing: &impl Backing,
) -> Result<(), Error> {
    let physical = device.physical_device();
    physical.require_version(vk::API_VERSION_1_1)?;
    let mut external_info = vk::PhysicalDeviceExternalImageFormatInfo::default();
    let mut info = vk::PhysicalDeviceImageFormatInfo2::default()
        .format(format)
        .ty(vk::ImageType::TYPE_2D)
        .tiling(vk::ImageTiling::OPTIMAL)
        .usage(usage);
    if let Some(handle_type) = backing.handle_type() {
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
    if !shared.contains(backing.required_features()) {
        return Err(Error::DeviceCannotShare);
    }
    Ok(())
}
