// SPDX-License-Identifier: GPL-3.0-only
//! Reads the Vulkan properties of AOSP hardware buffers and imports the buffers as the memory of Vulkan images,
//! through `VK_ANDROID_external_memory_android_hardware_buffer`.

use super::HardwareBufferProperties;
use crate::Error;
use crate::vulkan::buffer_format::ImportFormat;
use ash::android::external_memory_android_hardware_buffer as hardware_buffer;
use ash::vk;
use lavashell_vulkan::{Device, ExtensionSet, ImageFormat, ImportedMemory};
use ndk::hardware_buffer::{HardwareBuffer, HardwareBufferRef};
use std::ffi::CStr;

pub type FormatProperties = vk::AndroidHardwareBufferFormatPropertiesANDROID<'static>;

/// The result of `vkGetAndroidHardwareBufferPropertiesANDROID` for one hardware buffer: the memory of the buffer, or
/// `InvalidHandle` when the driver cannot import the buffer.
pub(super) enum BufferProperties {
    Usable(BufferMemory),
    InvalidHandle,
}

/// The allocation size, memory types and format properties that Vulkan returns for one hardware buffer. A reference to
/// the buffer is acquired with them, so the buffer exists as long as the properties do.
pub(super) struct BufferMemory {
    buffer: HardwareBufferRef,
    allocation_size: vk::DeviceSize,
    memory_type_bits: u32,
    format: FormatProperties,
}

impl BufferMemory {
    pub(super) fn format(&self) -> &FormatProperties {
        &self.format
    }

    pub(super) fn memory_type_bits(&self) -> u32 {
        self.memory_type_bits
    }
}

/// Reads the properties that the driver of `device` returns for `buffer`.
///
/// # Errors
/// Fails when the device lacks `VK_ANDROID_external_memory_android_hardware_buffer`, or when the driver fails to
/// return the properties for a reason other than a buffer that it cannot import. For such a buffer, the call returns
/// [`BufferProperties::InvalidHandle`].
pub(super) fn read_properties(
    device: &Device,
    buffer: &HardwareBuffer,
) -> Result<BufferProperties, Error> {
    if !device.contains(hardware_buffer::NAME) {
        return Err(Error::MissingExtension(hardware_buffer::NAME));
    }
    let functions = hardware_buffer::Device::new(device.instance().ash(), device.ash());
    let mut format = FormatProperties::default();
    let mut properties =
        vk::AndroidHardwareBufferPropertiesANDROID::default().push_next(&mut format);
    // SAFETY: ash 0.38 has no safe form of vkGetAndroidHardwareBufferPropertiesANDROID. The device has the extension
    // enabled, as the check above found. The buffer is live while it is borrowed. The chain of `properties` contains
    // the format properties defined by that extension.
    let read = unsafe {
        functions.get_android_hardware_buffer_properties(buffer.as_ptr().cast(), &mut properties)
    };
    match read {
        Ok(()) => Ok(BufferProperties::Usable(BufferMemory {
            buffer: buffer.acquire(),
            allocation_size: properties.allocation_size,
            memory_type_bits: properties.memory_type_bits,
            format,
        })),
        Err(vk::Result::ERROR_INVALID_EXTERNAL_HANDLE) => Ok(BufferProperties::InvalidHandle),
        Err(error) => Err(Error::HardwareBufferProperties(error)),
    }
}

// SAFETY: The Vulkan crate cannot check memory of a platform that it does not know, so its import trait is unsafe.
// `memory_chain` chains the import of the buffer that `vulkan` has a reference to, so the buffer is live through the
// allocation, and the handle type is the one of hardware buffers. `allocation` returns the allocation size and a memory
// type that `vkGetAndroidHardwareBufferPropertiesANDROID` returned for the buffer. For an external format,
// `image_chain` chains the external format that the same call returned.
unsafe impl ImportedMemory for HardwareBufferProperties {
    fn required_extensions(&self) -> &'static [&'static CStr] {
        &[hardware_buffer::NAME]
    }

    fn handle_type(&self) -> vk::ExternalMemoryHandleTypeFlags {
        vk::ExternalMemoryHandleTypeFlags::ANDROID_HARDWARE_BUFFER_ANDROID
    }

    fn format(&self) -> ImageFormat {
        match self.format {
            ImportFormat::Rgba => ImageFormat::Known(self.format.vulkan_format()),
            ImportFormat::External(_) => ImageFormat::External,
        }
    }

    /// Returns the allocation size that Vulkan returned for the buffer and the lowest memory type index that can
    /// contain the buffer.
    fn allocation(
        &self,
        _requirements: &vk::MemoryRequirements,
    ) -> Result<(vk::DeviceSize, u32), lavashell_vulkan::Error> {
        match self.vulkan.memory_type_bits {
            0 => Err(lavashell_vulkan::Error::NoMemoryType),
            bits => Ok((self.vulkan.allocation_size, bits.trailing_zeros())),
        }
    }

    fn memory_chain(
        &self,
        info: vk::MemoryAllocateInfo<'_>,
        allocate: impl FnOnce(
            &vk::MemoryAllocateInfo<'_>,
        ) -> Result<vk::DeviceMemory, lavashell_vulkan::Error>,
    ) -> Result<vk::DeviceMemory, lavashell_vulkan::Error> {
        let mut import = vk::ImportAndroidHardwareBufferInfoANDROID::default()
            .buffer(self.vulkan.buffer.as_ptr().cast());
        allocate(&info.push_next(&mut import))
    }

    fn image_chain(
        &self,
        info: vk::ImageCreateInfo<'_>,
        create: impl FnOnce(&vk::ImageCreateInfo<'_>) -> Result<vk::Image, lavashell_vulkan::Error>,
    ) -> Result<vk::Image, lavashell_vulkan::Error> {
        match self.format {
            ImportFormat::Rgba => create(&info),
            ImportFormat::External(_) => {
                let mut external = vk::ExternalFormatANDROID::default()
                    .external_format(self.vulkan.format.external_format);
                create(&info.push_next(&mut external))
            }
        }
    }
}
