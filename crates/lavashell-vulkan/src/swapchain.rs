// SPDX-License-Identifier: GPL-3.0-only
mod request;

use crate::{Device, Error, ExtensionSet, Presentation, Semaphore, Surface};
use ash::vk;
use num_traits::AsPrimitive;
pub use request::SwapchainRequest;
use std::sync::Arc;
use std::time::Duration;

/// A swapchain for a surface. Dropping it waits until the queue is idle, because a pending present may still read its
/// images.
pub struct Swapchain {
    handle: vk::SwapchainKHR,
    images: Vec<vk::Image>,
    request: SwapchainRequest,
    functions: ash::khr::swapchain::Device,
    device: Arc<Device>,
    surface: Arc<Surface>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fit {
    Optimal,
    Suboptimal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Acquisition {
    Image { index: u32, fit: Fit },
    NotReady,
    OutOfDate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Presented {
    Shown(Fit),
    OutOfDate,
}

/// A CIE 1931 xy chromaticity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Chromaticity {
    pub x: f64,
    pub y: f64,
}

/// The HDR metadata that describes the display on which content was mastered. The primaries and the white point are
/// CIE 1931 xy chromaticities. The luminances and the light levels of the content are in nits.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MasteringDisplay {
    pub red: Chromaticity,
    pub green: Chromaticity,
    pub blue: Chromaticity,
    pub white: Chromaticity,
    pub max_luminance: f64,
    pub min_luminance: f64,
    pub max_content_light_level: f64,
    pub max_frame_average_light_level: f64,
}

impl Swapchain {
    /// Creates a swapchain for `surface` that replaces `old`, when `old` is given.
    ///
    /// # Errors
    /// Fails when the device lacks `VK_KHR_swapchain` or cannot present to the surface, when the surface does not
    /// support the request, when `old` belongs to another surface, or when the driver cannot create the swapchain.
    pub fn new(
        device: &Arc<Device>,
        surface: &Arc<Surface>,
        request: SwapchainRequest,
        old: Option<&Self>,
    ) -> Result<Self, Error> {
        device.require(&[ash::khr::swapchain::NAME])?;
        if surface.presentation(device)? == Presentation::Unsupported {
            return Err(Error::SurfacePresentation);
        }
        let capabilities = surface.capabilities(device)?;
        let formats = surface.formats(device)?;
        let modes = surface.present_modes(device)?;
        request
            .check(&capabilities, &formats, &modes)
            .map_err(Error::SwapchainRequest)?;
        if old.is_some_and(|old| {
            !Arc::ptr_eq(&old.surface, surface) || !Arc::ptr_eq(&old.device, device)
        }) {
            return Err(Error::ForeignObject);
        }
        let info = request
            .info(surface.handle())
            .old_swapchain(old.map_or(vk::SwapchainKHR::null(), |old| old.handle));
        let functions = ash::khr::swapchain::Device::new(device.instance().ash(), device.ash());
        // SAFETY: ash 0.38 has no safe form of vkCreateSwapchainKHR. The device has the extension enabled, as
        // `require` checked. The surface and the old swapchain belong to the instance of the device. The queue family
        // of the device can present to the surface. `check` found the request supported by the surface.
        let handle =
            unsafe { functions.create_swapchain(&info, None) }.map_err(Error::CreateSwapchain)?;
        let mut swapchain = Self {
            handle,
            images: Vec::new(),
            request,
            functions,
            device: device.clone(),
            surface: surface.clone(),
        };
        // SAFETY: ash 0.38 has no safe form of vkGetSwapchainImagesKHR. The swapchain was just created on the live
        // device.
        let images = unsafe { swapchain.functions.get_swapchain_images(handle) };
        swapchain.images = images.map_err(Error::SwapchainImages)?;
        Ok(swapchain)
    }

    #[must_use]
    pub fn images(&self) -> &[vk::Image] {
        &self.images
    }

    #[must_use]
    pub fn request(&self) -> &SwapchainRequest {
        &self.request
    }

    /// Acquires the next image of the swapchain without waiting. For an image that is ready, the queue signals
    /// `semaphore` once the image can be written.
    ///
    /// # Errors
    /// Fails when the semaphore belongs to another device, or the driver fails.
    ///
    /// # Safety
    /// `semaphore` must be unsignaled, with no signal or wait pending.
    pub unsafe fn acquire(&mut self, semaphore: &Semaphore) -> Result<Acquisition, Error> {
        self.device.same(semaphore.device())?;
        // SAFETY: ash 0.38 has no safe form of vkAcquireNextImageKHR. The swapchain is live and borrowed exclusively.
        // The semaphore belongs to the device of the swapchain, as `same` checked. The caller guarantees that the
        // semaphore is unsignaled. The timeout is zero so that an acquire never blocks the calling thread.
        let acquired = unsafe {
            (self.functions).acquire_next_image(
                self.handle,
                0,
                semaphore.handle(),
                vk::Fence::null(),
            )
        };
        match acquired {
            Ok((index, suboptimal)) => Ok(Acquisition::Image {
                index,
                fit: fit(suboptimal),
            }),
            Err(vk::Result::NOT_READY | vk::Result::TIMEOUT) => Ok(Acquisition::NotReady),
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => Ok(Acquisition::OutOfDate),
            Err(error) => Err(Error::Acquire(error)),
        }
    }

    /// Presents image `index` once `wait` is signaled. When `time` is given, the image is shown no earlier than
    /// `time`, measured on the clock used by `VK_GOOGLE_display_timing`.
    ///
    /// # Errors
    /// Fails when the semaphore belongs to another device, when `time` is given and the device lacks
    /// `VK_GOOGLE_display_timing`, when `time` does not fit in the 64-bit nanoseconds used by that extension, or when
    /// the driver fails.
    ///
    /// # Safety
    /// Image `index` must be acquired and not presented since. A signal of `wait` must be pending, submitted after the
    /// writes to the image, and no other wait may consume that signal.
    pub unsafe fn present(
        &mut self,
        index: u32,
        wait: &Semaphore,
        time: Option<Duration>,
    ) -> Result<Presented, Error> {
        self.device.same(wait.device())?;
        if time.is_some() {
            self.device.require(&[ash::google::display_timing::NAME])?;
        }
        let nanoseconds = time.map(|time| u64::try_from(time.as_nanos()));
        let times =
            (nanoseconds.transpose().map_err(Error::PresentTime)?).map(|desired_present_time| {
                [vk::PresentTimeGOOGLE {
                    present_id: 0,
                    desired_present_time,
                }]
            });
        let mut timing =
            (times.as_ref()).map(|times| vk::PresentTimesInfoGOOGLE::default().times(times));
        let swapchains = [self.handle];
        let indices = [index];
        let waits = [wait.handle()];
        let mut info = vk::PresentInfoKHR::default()
            .wait_semaphores(&waits)
            .swapchains(&swapchains)
            .image_indices(&indices);
        if let Some(timing) = &mut timing {
            info = info.push_next(timing);
        }
        // SAFETY: ash 0.38 has no safe form of vkQueuePresentKHR. The lock in `with_queue` synchronizes the queue. The
        // swapchain is borrowed exclusively. The semaphore belongs to the device of the swapchain, as `same` checked.
        // The present time is chained only when the device has the display timing extension enabled. The caller
        // guarantees that the image is acquired and that the semaphore has a pending signal.
        let presented = self
            .device
            .with_queue(|queue| unsafe { self.functions.queue_present(queue, &info) });
        match presented {
            Ok(suboptimal) => Ok(Presented::Shown(fit(suboptimal))),
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => Ok(Presented::OutOfDate),
            Err(error) => Err(Error::Present(error)),
        }
    }

    /// # Errors
    /// Fails when the device lacks `VK_EXT_hdr_metadata`, or when a number of `display` lies beyond the range of a
    /// Vulkan float.
    pub fn set_mastering_display(&self, display: &MasteringDisplay) -> Result<(), Error> {
        self.device.require(&[ash::ext::hdr_metadata::NAME])?;
        let functions =
            ash::ext::hdr_metadata::Device::new(self.device.instance().ash(), self.device.ash());
        let xy = |color: Chromaticity| -> Result<vk::XYColorEXT, Error> {
            Ok(vk::XYColorEXT {
                x: vulkan_float(color.x)?,
                y: vulkan_float(color.y)?,
            })
        };
        let metadata = vk::HdrMetadataEXT::default()
            .display_primary_red(xy(display.red)?)
            .display_primary_green(xy(display.green)?)
            .display_primary_blue(xy(display.blue)?)
            .white_point(xy(display.white)?)
            .max_luminance(vulkan_float(display.max_luminance)?)
            .min_luminance(vulkan_float(display.min_luminance)?)
            .max_content_light_level(vulkan_float(display.max_content_light_level)?)
            .max_frame_average_light_level(vulkan_float(display.max_frame_average_light_level)?);
        // SAFETY: ash 0.38 has no safe form of vkSetHdrMetadataEXT. The swapchain belongs to the live device, which
        // has the extension enabled, as `require` checked.
        unsafe { functions.set_hdr_metadata(&[self.handle], &[metadata]) };
        Ok(())
    }
}

impl Drop for Swapchain {
    fn drop(&mut self) {
        if let Err(error) = self.device.wait_queue_idle() {
            log::error!("a Vulkan swapchain is destroyed without finishing its work: {error}");
        }
        // SAFETY: ash 0.38 has no safe form of vkDestroySwapchainKHR. The queue that uses the images is idle. The
        // surface and the device outlive the swapchain, because the swapchain contains an `Arc` of each.
        unsafe { self.functions.destroy_swapchain(self.handle, None) };
    }
}

fn fit(suboptimal: bool) -> Fit {
    if suboptimal {
        Fit::Suboptimal
    } else {
        Fit::Optimal
    }
}

/// Converts `value` to the nearest `f32`, the float type used by Vulkan. An infinity or a NaN converts to an
/// infinity or a NaN. A finite value beyond the range of `f32` fails.
fn vulkan_float(value: f64) -> Result<f32, Error> {
    let narrowed: f32 = value.as_();
    if value.is_finite() && narrowed.is_infinite() {
        Err(Error::HdrMetadataRange(value))
    } else {
        Ok(narrowed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_finite_number_beyond_the_range_of_a_vulkan_float_fails_and_an_infinity_stays_one() {
        let bits = |value: f64| vulkan_float(value).map(f32::to_bits).ok();
        assert_eq!(bits(0.3127), Some(0.3127_f32.to_bits()));
        assert_eq!(bits(f64::INFINITY), Some(f32::INFINITY.to_bits()));
        assert!(matches!(
            vulkan_float(f64::MAX),
            Err(Error::HdrMetadataRange(value)) if value.to_bits() == f64::MAX.to_bits()
        ));
    }
}
