// SPDX-License-Identifier: GPL-3.0-only
use super::SurfaceTiming;
use crate::Error;
use ash::vk;
use lavashell_core::prelude::{Gamut, HdrMetadata, OutputInfo, OutputProfile, OutputRequest};
use lavashell_vulkan::{
    Chromaticity, Device, ExtensionSet, MasteringDisplay, Semaphore, Submission, Swapchain,
    SwapchainRequest, Window,
};
use skia_safe::gpu::vk as skvk;
use skia_safe::{ColorType, Surface};
use std::sync::Arc;

mod draw;
mod extent;
pub(super) mod present;

pub(super) use present::PresentationTime;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FrameOutcome {
    Shown,
    Pending,
    Stale,
}

/// Whether the Skia surface into which a target draws still contains the last frame drawn at a given size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceContents {
    Current,
    Outdated,
}

/// How the compositor combines a surface with what lies behind it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceAlpha {
    /// The surface covers what lies behind it.
    Opaque,
    /// The compositor blends the surface over what lies behind it by its premultiplied alpha.
    Transparent,
}

/// The semaphore that an acquisition signals, and the submission of the last frame that waited on that semaphore.
struct Slot {
    acquire: Semaphore,
    frame: Option<Submission>,
}

/// A swapchain, with one semaphore for each image that the presentation of the image waits on, and one slot for each
/// frame in flight.
struct Chain {
    swapchain: Swapchain,
    finished: Vec<Semaphore>,
    slots: Vec<Slot>,
    next_slot: usize,
    layouts: Vec<skvk::ImageLayout>,
    color: ColorType,
    requested_size: (u32, u32),
    info: OutputInfo,
    working: Option<Surface>,
}

/// The swapchain presentation to the surface of one window, the same on every platform. Create a target through
/// `VulkanRenderer::surface_target`.
pub struct SurfaceTarget {
    surface: Arc<lavashell_vulkan::Surface>,
    chain: Option<Chain>,
    alpha: SurfaceAlpha,
    stale: bool,
    request: OutputRequest,
    metadata: Option<HdrMetadata>,
    transform: Option<vk::SurfaceTransformFlagsKHR>,
    presentation: SurfaceTiming,
    /// Declared after the chain, because fields are dropped in declaration order. The Skia surface of the chain is
    /// therefore dropped while the device still exists, also after the renderer is dropped.
    device: Arc<Device>,
}

impl SurfaceTarget {
    pub(super) fn new(
        device: Arc<Device>,
        window: Arc<dyn Window>,
        alpha: SurfaceAlpha,
        transform: Option<vk::SurfaceTransformFlagsKHR>,
        presentation: SurfaceTiming,
    ) -> Result<Self, Error> {
        let surface = lavashell_vulkan::Surface::new(device.instance(), window)
            .map_err(Error::CreateSurface)?;
        let presents = surface.presentation(&device).map_err(Error::SurfaceQuery)?;
        if presents == lavashell_vulkan::Presentation::Unsupported {
            return Err(Error::SurfacePresentation);
        }
        Ok(Self {
            device,
            surface,
            chain: None,
            alpha,
            stale: true,
            request: OutputRequest::default(),
            metadata: None,
            transform,
            presentation,
        })
    }

    pub(super) fn belongs_to(&self, device: &Arc<Device>) -> bool {
        Arc::ptr_eq(&self.device, device)
    }

    #[must_use]
    pub fn output(&self) -> Option<OutputInfo> {
        self.chain.as_ref().map(|chain| chain.info)
    }

    /// The number of frames that the swapchain can have in flight, once the target has drawn into one.
    #[must_use]
    pub fn frames_in_flight(&self) -> Option<usize> {
        self.chain.as_ref().map(|chain| chain.slots.len())
    }

    /// Whether the Skia surface into which the target draws contains the last frame drawn at `size`.
    ///
    /// # Errors
    /// Fails when Vulkan cannot report the extent of the surface.
    pub fn contents(&self, size: (u32, u32)) -> Result<SurfaceContents, Error> {
        let drawn_at_size = !self.stale
            && (self.chain.as_ref())
                .is_some_and(|chain| chain.requested_size == size && chain.working.is_some());
        if drawn_at_size && self.surface_extent()? == extent::Extent::Kept {
            Ok(SurfaceContents::Current)
        } else {
            Ok(SurfaceContents::Outdated)
        }
    }

    pub fn request_output(&mut self, request: OutputRequest) {
        if self.request != request {
            self.request = request;
            self.stale = true;
        }
    }

    /// # Errors
    /// Fails when the metadata is invalid.
    pub fn hdr_metadata(&mut self, metadata: HdrMetadata) -> Result<(), Error> {
        if !metadata.valid() {
            return Err(Error::InvalidHdrMetadata);
        }
        self.metadata = Some(metadata);
        self.stale = true;
        Ok(())
    }

    /// A new chain of `size` that replaces `old`. The swapchain of `old` is retired also when the new chain cannot be
    /// created.
    fn create_chain(&self, size: (u32, u32), old: Option<Chain>) -> Result<Chain, Error> {
        let capabilities =
            (self.surface.capabilities(&self.device)).map_err(Error::SurfaceQuery)?;
        let formats = (self.surface.formats(&self.device)).map_err(Error::SurfaceQuery)?;
        let selected =
            crate::color::select(&formats, self.request).ok_or(Error::NoSurfaceFormat)?;
        let request = SwapchainRequest {
            min_image_count: image_count(&capabilities),
            format: selected.format,
            extent: extent(&capabilities, size),
            usage: vk::ImageUsageFlags::COLOR_ATTACHMENT
                | vk::ImageUsageFlags::TRANSFER_SRC
                | vk::ImageUsageFlags::TRANSFER_DST,
            transform: self.transform.unwrap_or(capabilities.current_transform),
            composite_alpha: composite_alpha(&capabilities, self.alpha)
                .ok_or(Error::NoCompositeAlpha)?,
            // FIFO is the only present mode that Vulkan guarantees.
            present_mode: vk::PresentModeKHR::FIFO,
        };
        let replaced = old.as_ref().map(|old| &old.swapchain);
        let swapchain = Swapchain::new(&self.device, &self.surface, request, replaced)
            .map_err(Error::CreateSwapchain)?;
        drop(old);
        if selected.info.profile == OutputProfile::Hdr10
            && self.device.contains(ash::ext::hdr_metadata::NAME)
            && let Some(metadata) = self.metadata
        {
            (swapchain.set_mastering_display(&mastering_display(metadata)))
                .map_err(Error::SetHdrMetadata)?;
        }
        let images = swapchain.images().len();
        let semaphores = || {
            (0..images)
                .map(|_| Semaphore::new(&self.device))
                .collect::<Result<Vec<_>, _>>()
                .map_err(Error::CreateSemaphore)
        };
        let slots = (semaphores()?.into_iter())
            .map(|acquire| Slot {
                acquire,
                frame: None,
            })
            .collect();
        Ok(Chain {
            finished: semaphores()?,
            slots,
            next_slot: 0,
            layouts: vec![skvk::ImageLayout::UNDEFINED; images],
            color: selected.color,
            requested_size: size,
            info: selected.info,
            working: None,
            swapchain,
        })
    }
}

/// The number of swapchain images: one more than the minimum that the presentation engine may use for itself, so
/// that an acquisition can always complete. For a surface with a maximum image count, the number is at most that
/// maximum.
fn image_count(capabilities: &vk::SurfaceCapabilitiesKHR) -> u32 {
    let count = capabilities.min_image_count.saturating_add(1);
    if capabilities.max_image_count == 0 {
        count
    } else {
        count.min(capabilities.max_image_count)
    }
}

/// The current extent of the surface, or `size` clamped to the image extent limits of the surface when the surface
/// lets the swapchain choose the extent.
fn extent(capabilities: &vk::SurfaceCapabilitiesKHR, size: (u32, u32)) -> vk::Extent2D {
    let (minimum, maximum) = (capabilities.min_image_extent, capabilities.max_image_extent);
    if capabilities.current_extent.width == u32::MAX {
        vk::Extent2D {
            width: size.0.clamp(minimum.width, maximum.width),
            height: size.1.clamp(minimum.height, maximum.height),
        }
    } else {
        capabilities.current_extent
    }
}

/// The first composite alpha mode supported by the surface among the modes that show the target as it is drawn. A
/// transparent target never uses `OPAQUE`, which would ignore the alpha of the target.
fn composite_alpha(
    capabilities: &vk::SurfaceCapabilitiesKHR,
    alpha: SurfaceAlpha,
) -> Option<vk::CompositeAlphaFlagsKHR> {
    let candidates: &[_] = match alpha {
        SurfaceAlpha::Transparent => &[
            vk::CompositeAlphaFlagsKHR::PRE_MULTIPLIED,
            vk::CompositeAlphaFlagsKHR::INHERIT,
        ],
        SurfaceAlpha::Opaque => &[
            vk::CompositeAlphaFlagsKHR::OPAQUE,
            vk::CompositeAlphaFlagsKHR::PRE_MULTIPLIED,
            vk::CompositeAlphaFlagsKHR::INHERIT,
        ],
    };
    (candidates.iter().copied())
        .find(|alpha| capabilities.supported_composite_alpha.contains(*alpha))
}

/// The mastering display of `metadata`: the primaries that define its gamut and the D65 white point, in CIE 1931 xy
/// chromaticities.
fn mastering_display(metadata: HdrMetadata) -> MasteringDisplay {
    let xy = |[x, y]: [f64; 2]| Chromaticity { x, y };
    let [red, green, blue] = match metadata.mastering_gamut {
        Gamut::Srgb => [[0.640, 0.330], [0.300, 0.600], [0.150, 0.060]],
        Gamut::DisplayP3 => [[0.680, 0.320], [0.265, 0.690], [0.150, 0.060]],
        Gamut::Rec2020 => [[0.708, 0.292], [0.170, 0.797], [0.131, 0.046]],
    };
    MasteringDisplay {
        red: xy(red),
        green: xy(green),
        blue: xy(blue),
        white: xy([0.3127, 0.329]),
        max_luminance: metadata.max_luminance,
        min_luminance: metadata.min_luminance,
        max_content_light_level: metadata.max_content_light_level,
        max_frame_average_light_level: metadata.max_frame_average_light_level,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capabilities(min_image_count: u32, max_image_count: u32) -> vk::SurfaceCapabilitiesKHR {
        vk::SurfaceCapabilitiesKHR {
            min_image_count,
            max_image_count,
            ..Default::default()
        }
    }

    #[test]
    fn a_swapchain_has_one_image_beyond_the_minimum() {
        assert_eq!(image_count(&capabilities(2, 8)), 3);
        assert_eq!(image_count(&capabilities(2, 0)), 3);
    }

    #[test]
    fn a_swapchain_keeps_to_the_maximum_of_the_surface() {
        assert_eq!(image_count(&capabilities(3, 3)), 3);
    }

    #[test]
    fn an_opaque_target_prefers_opaque_and_a_transparent_target_never_takes_it() {
        let every = vk::SurfaceCapabilitiesKHR {
            supported_composite_alpha: vk::CompositeAlphaFlagsKHR::OPAQUE
                | vk::CompositeAlphaFlagsKHR::PRE_MULTIPLIED,
            ..Default::default()
        };
        assert_eq!(
            composite_alpha(&every, SurfaceAlpha::Opaque),
            Some(vk::CompositeAlphaFlagsKHR::OPAQUE)
        );
        assert_eq!(
            composite_alpha(&every, SurfaceAlpha::Transparent),
            Some(vk::CompositeAlphaFlagsKHR::PRE_MULTIPLIED)
        );
        let opaque = vk::SurfaceCapabilitiesKHR {
            supported_composite_alpha: vk::CompositeAlphaFlagsKHR::OPAQUE,
            ..Default::default()
        };
        assert_eq!(composite_alpha(&opaque, SurfaceAlpha::Transparent), None);
    }

    #[test]
    fn a_surface_without_a_current_extent_takes_the_size_within_its_limits() {
        let open = vk::SurfaceCapabilitiesKHR {
            current_extent: vk::Extent2D {
                width: u32::MAX,
                height: u32::MAX,
            },
            min_image_extent: vk::Extent2D {
                width: 1,
                height: 1,
            },
            max_image_extent: vk::Extent2D {
                width: 640,
                height: 480,
            },
            ..Default::default()
        };
        assert_eq!(
            extent(&open, (800, 200)),
            vk::Extent2D {
                width: 640,
                height: 200
            }
        );
        let fixed = vk::SurfaceCapabilitiesKHR {
            current_extent: vk::Extent2D {
                width: 320,
                height: 240,
            },
            ..open
        };
        assert_eq!(
            extent(&fixed, (800, 200)),
            vk::Extent2D {
                width: 320,
                height: 240
            }
        );
    }
}
