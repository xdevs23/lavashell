// SPDX-License-Identifier: GPL-3.0-only
//! The memory of AOSP hardware buffers, which the renderer imports through
//! `VK_ANDROID_external_memory_android_hardware_buffer`.

use ash::android::external_memory_android_hardware_buffer as hardware_buffer;
use ash::vk;
use lavashell_core::prelude::PixelSize;
use lavashell_renderer::{
    Error as RendererError, ForeignAlpha, ForeignFormat, ForeignImporter, ForeignMemory,
    VulkanRenderer,
};
use lavashell_vulkan::{Device, ExtensionSet};
use ndk::hardware_buffer::{HardwareBuffer, HardwareBufferRef};
use std::ffi::CStr;

/// A failure to read the Vulkan properties of a hardware buffer.
#[derive(Debug, thiserror::Error)]
pub enum PropertiesError {
    #[error("the Vulkan device cannot import hardware buffers")]
    Extension(#[source] lavashell_vulkan::Error),
    #[error("cannot read the Vulkan properties of a hardware buffer")]
    Read(#[source] vk::Result),
    #[error("the Vulkan driver cannot import the hardware buffer")]
    Unimportable,
}

/// The allocation size, memory types and format that Vulkan returns for one hardware buffer.
struct Properties {
    allocation_size: vk::DeviceSize,
    memory_type_bits: u32,
    format: ForeignFormat,
}

/// A hardware buffer with the properties that Vulkan returns for it. A reference to the buffer is acquired with them,
/// so the buffer exists as long as this value does.
pub struct HardwareBufferMemory {
    buffer: HardwareBufferRef,
    properties: Properties,
    size: PixelSize,
    alpha: ForeignAlpha,
}

impl HardwareBufferMemory {
    /// An importer of hardware buffers on the device used by `gpu`.
    ///
    /// # Errors
    /// Fails when the device lacks an extension that the import of hardware buffers needs.
    pub fn importer(gpu: &VulkanRenderer) -> Result<ForeignImporter, RendererError> {
        gpu.foreign_importer::<Self>()
    }

    /// Reads the memory of `buffer`, described with `size` and `alpha`, on the device used by `importer`. Returns
    /// `None` for a buffer that the driver cannot import.
    ///
    /// # Errors
    /// Fails as `read_properties` does for an importable buffer.
    pub fn read(
        importer: &ForeignImporter,
        buffer: &HardwareBuffer,
        size: PixelSize,
        alpha: ForeignAlpha,
    ) -> Result<Option<Self>, PropertiesError> {
        match read_properties(importer.device(), buffer) {
            Ok(properties) => Ok(Some(Self {
                buffer: buffer.acquire(),
                properties,
                size,
                alpha,
            })),
            Err(PropertiesError::Unimportable) => Ok(None),
            Err(error) => Err(error),
        }
    }
}

/// The format reported for `buffer` by the device used by `importer`.
///
/// # Errors
/// Fails as `read_properties` does.
pub fn read_format(
    importer: &ForeignImporter,
    buffer: &HardwareBuffer,
) -> Result<ForeignFormat, PropertiesError> {
    read_properties(importer.device(), buffer).map(|properties| properties.format)
}

/// Reads the properties that the driver of `device` returns for `buffer`.
///
/// # Errors
/// Fails when the device lacks `VK_ANDROID_external_memory_android_hardware_buffer`, when the driver cannot import the
/// buffer, or when the driver fails to return the properties.
fn read_properties(
    device: &Device,
    buffer: &HardwareBuffer,
) -> Result<Properties, PropertiesError> {
    device
        .require(&[hardware_buffer::NAME])
        .map_err(PropertiesError::Extension)?;
    let functions = hardware_buffer::Device::new(device.instance().ash(), device.ash());
    let mut format = vk::AndroidHardwareBufferFormatPropertiesANDROID::default();
    let mut properties =
        vk::AndroidHardwareBufferPropertiesANDROID::default().push_next(&mut format);
    // SAFETY: ash 0.38 has no safe form of vkGetAndroidHardwareBufferPropertiesANDROID. The device has the extension
    // enabled, as `require` checked. The buffer is live while it is borrowed. The chain of `properties` contains the
    // format properties defined by that extension.
    let read = unsafe {
        functions.get_android_hardware_buffer_properties(buffer.as_ptr().cast(), &mut properties)
    };
    match read {
        Ok(()) => Ok(Properties {
            allocation_size: properties.allocation_size,
            memory_type_bits: properties.memory_type_bits,
            format: foreign_format(&format),
        }),
        Err(vk::Result::ERROR_INVALID_EXTERNAL_HANDLE) => Err(PropertiesError::Unimportable),
        Err(error) => Err(PropertiesError::Read(error)),
    }
}

fn foreign_format(format: &vk::AndroidHardwareBufferFormatPropertiesANDROID<'_>) -> ForeignFormat {
    ForeignFormat {
        format: format.format,
        external_format: format.external_format,
        features: format.format_features,
        components: format.sampler_ycbcr_conversion_components,
        model: format.suggested_ycbcr_model,
        range: format.suggested_ycbcr_range,
        chroma_offsets: [
            format.suggested_x_chroma_offset,
            format.suggested_y_chroma_offset,
        ],
    }
}

// SAFETY: The renderer cannot check memory of a platform that it does not know, so its trait for foreign memory is
// unsafe. `memory_chain` chains the import of the buffer referenced by this value, so the buffer is live through the
// allocation, and the handle type is the one of hardware buffers. The allocation size, the memory type bits and the
// format are returned for the buffer by `vkGetAndroidHardwareBufferPropertiesANDROID`, and `external_format_chain`
// chains the external format from the same call. `size` is the size that AOSP describes for the buffer, which the
// driver imports at that size.
unsafe impl ForeignMemory for HardwareBufferMemory {
    const EXTENSIONS: &'static [&'static CStr] = &[hardware_buffer::NAME];

    fn handle_type(&self) -> vk::ExternalMemoryHandleTypeFlags {
        vk::ExternalMemoryHandleTypeFlags::ANDROID_HARDWARE_BUFFER_ANDROID
    }

    fn allocation_size(&self) -> vk::DeviceSize {
        self.properties.allocation_size
    }

    fn memory_type_bits(&self) -> u32 {
        self.properties.memory_type_bits
    }

    fn memory_chain(
        &self,
        info: vk::MemoryAllocateInfo<'_>,
        allocate: impl FnOnce(
            &vk::MemoryAllocateInfo<'_>,
        ) -> Result<vk::DeviceMemory, lavashell_vulkan::Error>,
    ) -> Result<vk::DeviceMemory, lavashell_vulkan::Error> {
        let mut import = vk::ImportAndroidHardwareBufferInfoANDROID::default()
            .buffer(self.buffer.as_ptr().cast());
        allocate(&info.push_next(&mut import))
    }

    fn external_format_chain(
        &self,
        info: vk::ImageCreateInfo<'_>,
        create: impl FnOnce(&vk::ImageCreateInfo<'_>) -> Result<vk::Image, lavashell_vulkan::Error>,
    ) -> Result<vk::Image, lavashell_vulkan::Error> {
        let mut external = vk::ExternalFormatANDROID::default()
            .external_format(self.properties.format.external_format);
        create(&info.push_next(&mut external))
    }

    fn size(&self) -> PixelSize {
        self.size
    }

    fn alpha(&self) -> ForeignAlpha {
        self.alpha
    }

    fn foreign_format(&self) -> &ForeignFormat {
        &self.properties.format
    }
}
