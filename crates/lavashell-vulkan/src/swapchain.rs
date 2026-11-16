// SPDX-License-Identifier: GPL-3.0-only
mod request;

use crate::sync::Fence;
use crate::{Device, Error, ExtensionSet, Presentation, Semaphore, Status, Surface};
use ash::vk;
pub use request::{SwapchainProperty, SwapchainRequest};
use std::ffi::CStr;
use std::sync::Arc;
use std::time::Duration;

struct Pending {
    fence: Fence,
    _semaphore: Arc<Semaphore>,
}

pub struct Swapchain {
    handle: vk::SwapchainKHR,
    images: Vec<vk::Image>,
    pending: Vec<Pending>,
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Chromaticity {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MasteringDisplay {
    pub red: Chromaticity,
    pub green: Chromaticity,
    pub blue: Chromaticity,
    pub white: Chromaticity,
    pub max_luminance: f32,
    pub min_luminance: f32,
    pub max_content_light_level: f32,
    pub max_frame_average_light_level: f32,
}

impl Swapchain {
    #[must_use]
    pub fn device_extensions() -> [&'static CStr; 2] {
        [
            ash::khr::swapchain::NAME,
            ash::ext::swapchain_maintenance1::NAME,
        ]
    }

    pub fn new(
        device: &Arc<Device>,
        surface: &Arc<Surface>,
        request: SwapchainRequest,
        old: Option<&Self>,
    ) -> Result<Self, Error> {
        device.require(&Self::device_extensions())?;
        if !device.features().swapchain_maintenance1 {
            return Err(Error::MissingSwapchainMaintenance);
        }
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
            pending: Vec::new(),
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

    /// # Safety
    /// `semaphore` must be unsignaled, with no signal or wait pending. Every acquired image must be presented, or the
    /// work that uses it must complete, before the swapchain drops.
    pub unsafe fn acquire(&mut self, semaphore: &Arc<Semaphore>) -> Result<Acquisition, Error> {
        self.device.require_same_device(semaphore.device())?;
        self.release_completed()?;
        let fence = Fence::new(&self.device)?;
        // SAFETY: ash 0.38 has no safe form of vkAcquireNextImageKHR. The swapchain is live and borrowed exclusively.
        // The semaphore and the fence belong to the device of the swapchain. The caller guarantees that the semaphore
        // is unsignaled, and the fence is new. The timeout is zero so that an acquire never blocks the calling
        // thread.
        let acquired = unsafe {
            (self.functions).acquire_next_image(self.handle, 0, semaphore.handle(), fence.handle())
        };
        match acquired {
            Ok((index, suboptimal)) => {
                self.hold_until_signaled(fence, semaphore);
                Ok(Acquisition::Image {
                    index,
                    fit: fit(suboptimal),
                })
            }
            Err(vk::Result::NOT_READY | vk::Result::TIMEOUT) => Ok(Acquisition::NotReady),
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => Ok(Acquisition::OutOfDate),
            Err(error) => Err(Error::Acquire(error)),
        }
    }

    /// # Safety
    /// Image `index` must be acquired and not presented since. A signal of `wait` must be pending, submitted after the
    /// writes to the image, and no other wait may consume that signal.
    pub unsafe fn present(
        &mut self,
        index: u32,
        wait: &Arc<Semaphore>,
        time: Option<Duration>,
    ) -> Result<Presented, Error> {
        self.device.require_same_device(wait.device())?;
        self.release_completed()?;
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
        let fence = Fence::new(&self.device)?;
        let fences = [fence.handle()];
        let mut completion = vk::SwapchainPresentFenceInfoEXT::default().fences(&fences);
        let mut info = vk::PresentInfoKHR::default()
            .wait_semaphores(&waits)
            .swapchains(&swapchains)
            .image_indices(&indices)
            .push_next(&mut completion);
        if let Some(timing) = &mut timing {
            info = info.push_next(timing);
        }
        // SAFETY: ash 0.38 has no safe form of vkQueuePresentKHR. The lock in `with_queue` synchronizes the queue. The
        // swapchain is borrowed exclusively. The semaphore belongs to the device of the swapchain, as
        // `require_same_device` checked. The fence is new, and the device has the swapchain maintenance feature
        // enabled, as `new` checked. The present time is chained only when the device has the display timing extension
        // enabled. The caller guarantees that the image is acquired and that the semaphore has a pending signal.
        let presented = self
            .device
            .with_queue(|queue| unsafe { self.functions.queue_present(queue, &info) });
        match presented {
            Ok(suboptimal) => {
                self.hold_until_signaled(fence, wait);
                Ok(Presented::Shown(fit(suboptimal)))
            }
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                self.hold_until_signaled(fence, wait);
                Ok(Presented::OutOfDate)
            }
            Err(error) => Err(Error::Present(error)),
        }
    }

    fn hold_until_signaled(&mut self, fence: Fence, semaphore: &Arc<Semaphore>) {
        self.pending.push(Pending {
            fence,
            _semaphore: semaphore.clone(),
        });
    }

    fn release_completed(&mut self) -> Result<(), Error> {
        let mut status = Ok(());
        self.pending.retain(|pending| match pending.fence.status() {
            Ok(Status::Complete) => false,
            Ok(Status::Pending) => true,
            Err(error) => {
                status = Err(error);
                true
            }
        });
        status
    }

    pub fn set_mastering_display(&self, display: &MasteringDisplay) -> Result<(), Error> {
        self.device.require(&[ash::ext::hdr_metadata::NAME])?;
        let functions =
            ash::ext::hdr_metadata::Device::new(self.device.instance().ash(), self.device.ash());
        let xy = |color: Chromaticity| vk::XYColorEXT {
            x: color.x,
            y: color.y,
        };
        let metadata = vk::HdrMetadataEXT::default()
            .display_primary_red(xy(display.red))
            .display_primary_green(xy(display.green))
            .display_primary_blue(xy(display.blue))
            .white_point(xy(display.white))
            .max_luminance(display.max_luminance)
            .min_luminance(display.min_luminance)
            .max_content_light_level(display.max_content_light_level)
            .max_frame_average_light_level(display.max_frame_average_light_level);
        // SAFETY: ash 0.38 has no safe form of vkSetHdrMetadataEXT. The swapchain belongs to the live device, which
        // has the extension enabled, as `require` checked.
        unsafe { functions.set_hdr_metadata(&[self.handle], &[metadata]) };
        Ok(())
    }
}

impl Drop for Swapchain {
    fn drop(&mut self) {
        for pending in &self.pending {
            if let Err(error) = pending.fence.wait() {
                log::error!(
                    "a Vulkan swapchain is destroyed before its operations finished: {error}"
                );
            }
        }
        // SAFETY: ash 0.38 has no safe form of vkDestroySwapchainKHR. The fence of every acquisition and presentation
        // signaled, so the presentation engine no longer uses the images or the semaphores of the swapchain, and the
        // callers of `acquire` completed the work that uses the images. The surface and the device outlive the
        // swapchain, because the swapchain contains an `Arc` of each.
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
