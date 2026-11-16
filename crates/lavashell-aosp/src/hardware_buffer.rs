// SPDX-License-Identifier: GPL-3.0-only

use ash::android::external_memory_android_hardware_buffer as hardware_buffer;
use ash::vk;
use lavashell_core::prelude::PixelSize;
use lavashell_renderer::{
    Error as RendererError, ForeignAlpha, ForeignFormat, ForeignImporter, ForeignMemory,
    ForeignMemoryDescription, VulkanRenderer,
};
use lavashell_vulkan::{Device, ExtensionSet};
use ndk::hardware_buffer::{HardwareBuffer, HardwareBufferRef};
use std::ffi::CStr;
use std::num::NonZeroU64;

#[derive(Debug, thiserror::Error)]
pub enum PropertiesError {
    #[error("the Vulkan device cannot import hardware buffers")]
    Extension(#[source] lavashell_vulkan::Error),
    #[error("cannot read the Vulkan properties of a hardware buffer")]
    Read(#[source] vk::Result),
    #[error("the Vulkan driver cannot import the hardware buffer")]
    Unimportable,
}

struct Properties {
    allocation_size: vk::DeviceSize,
    memory_type_bits: u32,
    format: ForeignFormat,
}

pub struct HardwareBufferMemory {
    buffer: HardwareBufferRef,
    description: ForeignMemoryDescription,
}

impl HardwareBufferMemory {
    pub fn importer(gpu: &VulkanRenderer) -> Result<ForeignImporter, RendererError> {
        gpu.foreign_importer::<Self>()
    }

    pub fn read(
        importer: &ForeignImporter,
        buffer: &HardwareBuffer,
        size: PixelSize,
        alpha: ForeignAlpha,
    ) -> Result<Option<Self>, PropertiesError> {
        match read_properties(importer.device(), buffer) {
            Ok(properties) => Ok(Some(Self {
                buffer: buffer.acquire(),
                description: ForeignMemoryDescription {
                    handle_type: vk::ExternalMemoryHandleTypeFlags::ANDROID_HARDWARE_BUFFER_ANDROID,
                    allocation_size: properties.allocation_size,
                    memory_type_bits: properties.memory_type_bits,
                    size,
                    alpha,
                    format: properties.format,
                },
            })),
            Err(PropertiesError::Unimportable) => Ok(None),
            Err(error) => Err(error),
        }
    }
}

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
    // SAFETY: ash 0.38 has no safe form of the call that reads the properties of a hardware buffer. The device has
    // the extension enabled, as `require` checked. The buffer is live while it is borrowed. The chain of `properties`
    // contains the format properties defined by that extension.
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
        external_format: NonZeroU64::new(format.external_format),
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
// allocation, and the described handle type is the one of hardware buffers. The described allocation size, memory
// type bits and format are returned for the buffer by the Vulkan call that reads its properties. The described size
// is the size that AOSP describes for the buffer, which the driver imports at that size.
unsafe impl ForeignMemory for HardwareBufferMemory {
    const EXTENSIONS: &'static [&'static CStr] = &[hardware_buffer::NAME];

    fn description(&self) -> &ForeignMemoryDescription {
        &self.description
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
}
