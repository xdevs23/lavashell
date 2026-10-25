// SPDX-License-Identifier: GPL-3.0-only
//! The Vulkan formats and usages of wgpu textures that Lavashell draws into or exports.

use crate::Error;
use ash::vk;
use lavashell_vulkan::ImageDescription;

/// A wgpu texture usage with the resource use of wgpu-hal and the Vulkan image usage that it stands for.
struct Usage {
    texture: wgpu::TextureUsages,
    hal: wgpu::TextureUses,
    image: vk::ImageUsageFlags,
}

static USAGES: [Usage; 5] = [
    Usage {
        texture: wgpu::TextureUsages::RENDER_ATTACHMENT,
        hal: wgpu::TextureUses::COLOR_TARGET,
        image: vk::ImageUsageFlags::COLOR_ATTACHMENT,
    },
    Usage {
        texture: wgpu::TextureUsages::TEXTURE_BINDING,
        hal: wgpu::TextureUses::RESOURCE,
        image: vk::ImageUsageFlags::SAMPLED,
    },
    Usage {
        texture: wgpu::TextureUsages::COPY_SRC,
        hal: wgpu::TextureUses::COPY_SRC,
        image: vk::ImageUsageFlags::TRANSFER_SRC,
    },
    Usage {
        texture: wgpu::TextureUsages::COPY_DST,
        hal: wgpu::TextureUses::COPY_DST,
        image: vk::ImageUsageFlags::TRANSFER_DST,
    },
    Usage {
        texture: wgpu::TextureUsages::STORAGE_BINDING,
        hal: wgpu::TextureUses::STORAGE_READ_WRITE,
        image: vk::ImageUsageFlags::STORAGE,
    },
];

/// The description of the Vulkan image that backs a wgpu texture with `format`, `usage` and `extent`, or an error for a
/// format or usage that has no Vulkan counterpart in this module.
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
    Ok(rows_of(usage)?.fold(vk::ImageUsageFlags::empty(), |flags, row| flags | row.image))
}

pub(crate) fn hal_uses(usage: wgpu::TextureUsages) -> Result<wgpu::TextureUses, Error> {
    Ok(rows_of(usage)?.fold(wgpu::TextureUses::empty(), |uses, row| uses | row.hal))
}

/// The rows of [`USAGES`] whose wgpu usage is set in `usage`. Returns an error with the bits of `usage` that no row
/// maps, so a conversion never drops a usage without an error.
fn rows_of(usage: wgpu::TextureUsages) -> Result<impl Iterator<Item = &'static Usage>, Error> {
    let unmapped = (USAGES.iter()).fold(usage, |rest, row| rest.difference(row.texture));
    if !unmapped.is_empty() {
        return Err(Error::UnsupportedUsage(unmapped));
    }
    Ok(USAGES.iter().filter(move |row| usage.contains(row.texture)))
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
