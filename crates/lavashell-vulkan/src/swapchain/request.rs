// SPDX-License-Identifier: GPL-3.0-only
use crate::SwapchainProperty;
use ash::vk;

/// The properties requested for a new swapchain. The swapchain always gets single-layer images in exclusive sharing
/// mode, clipped where other windows cover them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SwapchainRequest {
    pub min_image_count: u32,
    pub format: vk::SurfaceFormatKHR,
    pub extent: vk::Extent2D,
    pub usage: vk::ImageUsageFlags,
    pub transform: vk::SurfaceTransformFlagsKHR,
    pub composite_alpha: vk::CompositeAlphaFlagsKHR,
    pub present_mode: vk::PresentModeKHR,
}

impl SwapchainRequest {
    /// Fails with the first property of the request that the surface does not support.
    pub(super) fn check(
        &self,
        capabilities: &vk::SurfaceCapabilitiesKHR,
        formats: &[vk::SurfaceFormatKHR],
        modes: &[vk::PresentModeKHR],
    ) -> Result<(), SwapchainProperty> {
        let unlimited = capabilities.max_image_count == 0;
        let (minimum, maximum) = (capabilities.min_image_extent, capabilities.max_image_extent);
        let checks = [
            (
                self.min_image_count >= capabilities.min_image_count
                    && (unlimited || self.min_image_count <= capabilities.max_image_count),
                SwapchainProperty::ImageCount,
            ),
            (formats.contains(&self.format), SwapchainProperty::Format),
            (
                (minimum.width..=maximum.width).contains(&self.extent.width)
                    && (minimum.height..=maximum.height).contains(&self.extent.height),
                SwapchainProperty::Extent,
            ),
            (
                capabilities.supported_usage_flags.contains(self.usage),
                SwapchainProperty::Usage,
            ),
            (
                self.transform.as_raw().is_power_of_two()
                    && capabilities.supported_transforms.contains(self.transform),
                SwapchainProperty::Transform,
            ),
            (
                self.composite_alpha.as_raw().is_power_of_two()
                    && capabilities
                        .supported_composite_alpha
                        .contains(self.composite_alpha),
                SwapchainProperty::CompositeAlpha,
            ),
            (
                modes.contains(&self.present_mode),
                SwapchainProperty::PresentMode,
            ),
        ];
        match checks.into_iter().find(|(supported, _)| !supported) {
            Some((_, property)) => Err(property),
            None => Ok(()),
        }
    }

    pub(super) fn info(&self, surface: vk::SurfaceKHR) -> vk::SwapchainCreateInfoKHR<'static> {
        vk::SwapchainCreateInfoKHR::default()
            .surface(surface)
            .min_image_count(self.min_image_count)
            .image_format(self.format.format)
            .image_color_space(self.format.color_space)
            .image_extent(self.extent)
            .image_array_layers(1)
            .image_usage(self.usage)
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .pre_transform(self.transform)
            .composite_alpha(self.composite_alpha)
            .present_mode(self.present_mode)
            .clipped(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FORMAT: vk::SurfaceFormatKHR = vk::SurfaceFormatKHR {
        format: vk::Format::B8G8R8A8_UNORM,
        color_space: vk::ColorSpaceKHR::SRGB_NONLINEAR,
    };

    fn capabilities() -> vk::SurfaceCapabilitiesKHR {
        vk::SurfaceCapabilitiesKHR {
            min_image_count: 2,
            max_image_count: 4,
            min_image_extent: vk::Extent2D {
                width: 1,
                height: 1,
            },
            max_image_extent: vk::Extent2D {
                width: 640,
                height: 480,
            },
            supported_usage_flags: vk::ImageUsageFlags::COLOR_ATTACHMENT
                | vk::ImageUsageFlags::TRANSFER_DST,
            supported_transforms: vk::SurfaceTransformFlagsKHR::IDENTITY,
            supported_composite_alpha: vk::CompositeAlphaFlagsKHR::OPAQUE,
            ..Default::default()
        }
    }

    fn request() -> SwapchainRequest {
        SwapchainRequest {
            min_image_count: 3,
            format: FORMAT,
            extent: vk::Extent2D {
                width: 640,
                height: 480,
            },
            usage: vk::ImageUsageFlags::COLOR_ATTACHMENT,
            transform: vk::SurfaceTransformFlagsKHR::IDENTITY,
            composite_alpha: vk::CompositeAlphaFlagsKHR::OPAQUE,
            present_mode: vk::PresentModeKHR::FIFO,
        }
    }

    fn check(request: &SwapchainRequest) -> Result<(), SwapchainProperty> {
        request.check(&capabilities(), &[FORMAT], &[vk::PresentModeKHR::FIFO])
    }

    #[test]
    fn a_request_within_the_capabilities_passes() {
        assert_eq!(check(&request()), Ok(()));
    }

    #[test]
    fn each_unsupported_property_is_named() {
        let cases = [
            (
                SwapchainRequest {
                    min_image_count: 5,
                    ..request()
                },
                SwapchainProperty::ImageCount,
            ),
            (
                SwapchainRequest {
                    format: vk::SurfaceFormatKHR {
                        format: vk::Format::R8G8B8A8_UNORM,
                        ..FORMAT
                    },
                    ..request()
                },
                SwapchainProperty::Format,
            ),
            (
                SwapchainRequest {
                    extent: vk::Extent2D {
                        width: 641,
                        height: 1,
                    },
                    ..request()
                },
                SwapchainProperty::Extent,
            ),
            (
                SwapchainRequest {
                    usage: vk::ImageUsageFlags::SAMPLED,
                    ..request()
                },
                SwapchainProperty::Usage,
            ),
            (
                SwapchainRequest {
                    transform: vk::SurfaceTransformFlagsKHR::ROTATE_90,
                    ..request()
                },
                SwapchainProperty::Transform,
            ),
            (
                SwapchainRequest {
                    composite_alpha: vk::CompositeAlphaFlagsKHR::INHERIT,
                    ..request()
                },
                SwapchainProperty::CompositeAlpha,
            ),
            (
                SwapchainRequest {
                    present_mode: vk::PresentModeKHR::MAILBOX,
                    ..request()
                },
                SwapchainProperty::PresentMode,
            ),
        ];
        for (request, property) in cases {
            assert_eq!(check(&request), Err(property));
        }
    }

    #[test]
    fn a_surface_without_an_image_limit_accepts_any_count_above_its_minimum() {
        let unlimited = vk::SurfaceCapabilitiesKHR {
            max_image_count: 0,
            ..capabilities()
        };
        let request = SwapchainRequest {
            min_image_count: 9,
            ..request()
        };
        assert_eq!(
            request.check(&unlimited, &[FORMAT], &[vk::PresentModeKHR::FIFO]),
            Ok(())
        );
    }

    #[test]
    fn a_request_names_exactly_one_transform() {
        let both = vk::SurfaceTransformFlagsKHR::IDENTITY | vk::SurfaceTransformFlagsKHR::ROTATE_90;
        let request = SwapchainRequest {
            transform: both,
            ..request()
        };
        let rotating = vk::SurfaceCapabilitiesKHR {
            supported_transforms: both,
            ..capabilities()
        };
        let checked = request.check(&rotating, &[FORMAT], &[vk::PresentModeKHR::FIFO]);
        assert_eq!(checked, Err(SwapchainProperty::Transform));
    }
}
