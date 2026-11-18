// SPDX-License-Identifier: GPL-3.0-only
use super::SkiaFormat;
use crate::Error;
use ash::vk;
use lavashell_core::color;
use lavashell_core::prelude::{HdrMetadata, OutputInfo, OutputProfile, OutputRequest, PixelSize};
use lavashell_platform::{FrameOutcome, SurfaceAlpha};
use lavashell_renderer::{DrawFailure, scalars};
use lavashell_vulkan::{
    Chromaticity, Device, ExtensionSet, MasteringDisplay, Semaphore, Submission, Swapchain,
    SwapchainRequest, Window,
};
use skia_safe::Surface;
use skia_safe::gpu::vk as skvk;
use std::sync::Arc;

mod draw;
mod extent;
#[cfg(test)]
mod painted;
pub(super) mod present;

use present::PresentationPolicy;
pub(super) use present::PresentationTime;

pub(crate) enum DrawnFrame<T> {
    Skipped(SkippedFrame),
    Drawn { outcome: FrameOutcome, painted: T },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SkippedFrame {
    EmptySize,
    ImageNotReady,
    ChainOutOfDate,
}

impl SkippedFrame {
    pub(crate) fn outcome(self) -> FrameOutcome {
        match self {
            Self::EmptySize | Self::ImageNotReady => FrameOutcome::Pending,
            Self::ChainOutOfDate => FrameOutcome::Stale,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceContents {
    Current,
    Outdated,
}

struct Slot {
    acquire: Arc<Semaphore>,
    frame: Option<Submission>,
}

struct Chain {
    slots: Vec<Slot>,
    finished: Vec<Arc<Semaphore>>,
    swapchain: Swapchain,
    next_slot: usize,
    layouts: Vec<skvk::ImageLayout>,
    format: SkiaFormat,
    requested_size: PixelSize,
    info: OutputInfo,
    working: Option<Surface>,
}

pub struct SurfaceTarget {
    surface: Arc<lavashell_vulkan::Surface>,
    chain: Option<Chain>,
    alpha: SurfaceAlpha,
    stale: bool,
    request: OutputRequest,
    metadata: Option<HdrMetadata>,
    transform: Option<vk::SurfaceTransformFlagsKHR>,
    presentation: PresentationPolicy,
    device: Arc<Device>,
}

impl SurfaceTarget {
    pub(super) fn new(
        device: Arc<Device>,
        window: Arc<dyn Window>,
        alpha: SurfaceAlpha,
        transform: Option<vk::SurfaceTransformFlagsKHR>,
    ) -> Result<Self, Error> {
        let surface = lavashell_vulkan::Surface::new(device.instance(), window)
            .map_err(Error::CreateSurface)?;
        let presents = surface
            .presentation(&device)
            .map_err(Error::QuerySurfacePresentation)?;
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
            presentation: PresentationPolicy::Automatic,
        })
    }

    pub(super) fn belongs_to(&self, device: &Arc<Device>) -> bool {
        Arc::ptr_eq(&self.device, device)
    }

    #[must_use]
    pub fn output(&self) -> Option<OutputInfo> {
        self.chain.as_ref().map(|chain| chain.info)
    }

    #[must_use]
    pub fn frames_in_flight(&self) -> Option<usize> {
        self.chain.as_ref().map(|chain| chain.slots.len())
    }

    pub fn contents(&self, size: PixelSize) -> Result<SurfaceContents, Error> {
        let drawn_at_size = !self.stale
            && (self.chain.as_ref())
                .is_some_and(|chain| chain.requested_size == size && chain.working.is_some());
        if drawn_at_size && self.extent_change()? == extent::ExtentChange::Kept {
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

    pub fn hdr_metadata(&mut self, metadata: HdrMetadata) -> Result<(), Error> {
        if !metadata.valid() {
            return Err(Error::InvalidHdrMetadata);
        }
        self.metadata = Some(metadata);
        self.stale = true;
        Ok(())
    }

    fn create_chain(&self, size: PixelSize, old: Option<Chain>) -> Result<Chain, Error> {
        let capabilities =
            (self.surface.capabilities(&self.device)).map_err(Error::QuerySurfaceCapabilities)?;
        let formats = (self.surface.formats(&self.device)).map_err(Error::QuerySurfaceFormats)?;
        let selected =
            crate::color::select(&formats, self.request).ok_or(Error::NoSurfaceFormat)?;
        let request = SwapchainRequest {
            min_image_count: image_count(&capabilities),
            format: vk::SurfaceFormatKHR {
                format: selected.format.vulkan(),
                color_space: selected.color_space,
            },
            extent: extent(&capabilities, size),
            usage: vk::ImageUsageFlags::COLOR_ATTACHMENT
                | vk::ImageUsageFlags::TRANSFER_SRC
                | vk::ImageUsageFlags::TRANSFER_DST,
            transform: self.transform.unwrap_or(capabilities.current_transform),
            composite_alpha: composite_alpha(&capabilities, self.alpha)
                .ok_or(Error::NoCompositeAlpha)?,
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
            let display = mastering_display(metadata).map_err(Error::HdrMetadataRange)?;
            swapchain
                .set_mastering_display(&display)
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
            format: selected.format,
            requested_size: size,
            info: selected.info,
            working: None,
            swapchain,
        })
    }
}

fn image_count(capabilities: &vk::SurfaceCapabilitiesKHR) -> u32 {
    let count = capabilities.min_image_count.saturating_add(1);
    if capabilities.max_image_count == 0 {
        count
    } else {
        count.min(capabilities.max_image_count)
    }
}

fn extent(capabilities: &vk::SurfaceCapabilitiesKHR, size: PixelSize) -> vk::Extent2D {
    let (minimum, maximum) = (capabilities.min_image_extent, capabilities.max_image_extent);
    if capabilities.current_extent.width == u32::MAX {
        vk::Extent2D {
            width: size.width.clamp(minimum.width, maximum.width),
            height: size.height.clamp(minimum.height, maximum.height),
        }
    } else {
        capabilities.current_extent
    }
}

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

fn mastering_display(metadata: HdrMetadata) -> Result<MasteringDisplay, DrawFailure> {
    let xy = |color::Chromaticity { x, y }| scalars([x, y]).map(|[x, y]| Chromaticity { x, y });
    let primaries = metadata.mastering_gamut.primaries();
    let [
        max_luminance,
        min_luminance,
        max_content_light_level,
        max_frame_average_light_level,
    ] = scalars([
        metadata.max_luminance,
        metadata.min_luminance,
        metadata.max_content_light_level,
        metadata.max_frame_average_light_level,
    ])?;
    Ok(MasteringDisplay {
        red: xy(primaries.red)?,
        green: xy(primaries.green)?,
        blue: xy(primaries.blue)?,
        white: xy(primaries.white)?,
        max_luminance,
        min_luminance,
        max_content_light_level,
        max_frame_average_light_level,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lavashell_renderer::test_surfaces::TestSurface;

    const REQUESTED_SURFACE: TestSurface = TestSurface::new(800, 200);

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
            extent(&open, REQUESTED_SURFACE.pixel_size()),
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
            extent(&fixed, REQUESTED_SURFACE.pixel_size()),
            vk::Extent2D {
                width: 320,
                height: 240
            }
        );
    }
}
