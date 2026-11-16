// SPDX-License-Identifier: GPL-3.0-only

use crate::Error;
use ash::vk;
use lavashell_vulkan::ImageDescription;

struct UsageMapping {
    texture: wgpu::TextureUsages,
    hal: wgpu::TextureUses,
    image: vk::ImageUsageFlags,
}

static USAGE_MAPPINGS: [UsageMapping; 5] = [
    UsageMapping {
        texture: wgpu::TextureUsages::RENDER_ATTACHMENT,
        hal: wgpu::TextureUses::COLOR_TARGET,
        image: vk::ImageUsageFlags::COLOR_ATTACHMENT,
    },
    UsageMapping {
        texture: wgpu::TextureUsages::TEXTURE_BINDING,
        hal: wgpu::TextureUses::RESOURCE,
        image: vk::ImageUsageFlags::SAMPLED,
    },
    UsageMapping {
        texture: wgpu::TextureUsages::COPY_SRC,
        hal: wgpu::TextureUses::COPY_SRC,
        image: vk::ImageUsageFlags::TRANSFER_SRC,
    },
    UsageMapping {
        texture: wgpu::TextureUsages::COPY_DST,
        hal: wgpu::TextureUses::COPY_DST,
        image: vk::ImageUsageFlags::TRANSFER_DST,
    },
    UsageMapping {
        texture: wgpu::TextureUsages::STORAGE_BINDING,
        hal: wgpu::TextureUses::STORAGE_READ_WRITE,
        image: vk::ImageUsageFlags::STORAGE,
    },
];

pub(crate) fn image_description(
    format: wgpu::TextureFormat,
    usage: wgpu::TextureUsages,
    extent: vk::Extent2D,
) -> Result<ImageDescription, Error> {
    Ok(ImageDescription {
        format: vulkan_format(format)?,
        extent,
        usage: vulkan_usage(usage)?,
    })
}

pub(crate) fn vulkan_format(format: wgpu::TextureFormat) -> Result<vk::Format, Error> {
    match format {
        wgpu::TextureFormat::Rgba8Unorm => Ok(vk::Format::R8G8B8A8_UNORM),
        wgpu::TextureFormat::Rgba8UnormSrgb => Ok(vk::Format::R8G8B8A8_SRGB),
        wgpu::TextureFormat::Bgra8Unorm => Ok(vk::Format::B8G8R8A8_UNORM),
        wgpu::TextureFormat::Bgra8UnormSrgb => Ok(vk::Format::B8G8R8A8_SRGB),
        wgpu::TextureFormat::Rgba16Float => Ok(vk::Format::R16G16B16A16_SFLOAT),
        other => Err(Error::TextureFormat(other)),
    }
}

pub(crate) fn vulkan_usage(usage: wgpu::TextureUsages) -> Result<vk::ImageUsageFlags, Error> {
    let mappings = mappings_in(usage)?;
    Ok(mappings.fold(vk::ImageUsageFlags::empty(), |all, mapping| {
        all | mapping.image
    }))
}

pub(crate) fn hal_uses(usage: wgpu::TextureUsages) -> Result<wgpu::TextureUses, Error> {
    let mappings = mappings_in(usage)?;
    Ok(mappings.fold(wgpu::TextureUses::empty(), |all, mapping| all | mapping.hal))
}

fn mappings_in(
    usage: wgpu::TextureUsages,
) -> Result<impl Iterator<Item = &'static UsageMapping>, Error> {
    let unmapped =
        (USAGE_MAPPINGS.iter()).fold(usage, |rest, mapping| rest.difference(mapping.texture));
    if !unmapped.is_empty() {
        return Err(Error::UnsupportedUsage(unmapped));
    }
    Ok(USAGE_MAPPINGS
        .iter()
        .filter(move |mapping| usage.contains(mapping.texture)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_usage_without_a_vulkan_usage_is_an_error() {
        let atomic = wgpu::TextureUsages::STORAGE_ATOMIC;
        let refused = vulkan_usage(wgpu::TextureUsages::COPY_SRC | atomic);
        assert!(matches!(refused, Err(Error::UnsupportedUsage(bits)) if bits == atomic));
    }

    #[test]
    fn a_format_without_a_vulkan_target_is_an_error() {
        assert!(matches!(
            vulkan_format(wgpu::TextureFormat::R8Unorm),
            Err(Error::TextureFormat(wgpu::TextureFormat::R8Unorm))
        ));
    }
}
