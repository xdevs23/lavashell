// SPDX-License-Identifier: GPL-3.0-only

use crate::{ContextState, Error};
use ash::vk::{self, Handle};
use lavashell_enum_values::enum_with_values;
use lavashell_renderer::RasterMode;
use lavashell_vulkan::Device;
use skia_safe::gpu::{self, DirectContext, SurfaceOrigin, vk as skvk};
use skia_safe::{ColorSpace, ColorType, ImageInfo, Surface};
use std::ffi::{CStr, c_void};
use std::sync::Arc;

// https://github.com/rust-skia/skia/blob/m153-0.101.2/src/text/gpu/SubRunControl.cpp
const SKIA_LARGE_DISTANCE_FIELD_FONT_LIMIT: f32 = 162.0;
const EVERY_FONT_SIZE_AS_PATHS: f32 = f32::MIN_POSITIVE;

pub(super) struct SkiaDevice<'a> {
    entry: &'a ash::Entry,
    instance: &'a ash::Instance,
    physical: vk::PhysicalDevice,
    device: &'a ash::Device,
    queue: vk::Queue,
    family: u32,
    api_version: u32,
    instance_extensions: Vec<&'static str>,
    device_extensions: Vec<&'static str>,
    ycbcr_conversion: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum YcbcrConversion {
    Off,
    AsEnabledOnDevice,
}

impl<'a> SkiaDevice<'a> {
    pub fn of(device: &'a Device, ycbcr_conversion: YcbcrConversion) -> Result<Self, Error> {
        let instance = device.instance();
        let ycbcr_conversion = match ycbcr_conversion {
            YcbcrConversion::Off => false,
            YcbcrConversion::AsEnabledOnDevice => device.features().sampler_ycbcr_conversion,
        };
        // SAFETY: The handle only moves into the returned value, and `direct_context` is the one function that reads
        // it. The contract of `direct_context` is the contract of `queue_handle`: the submissions of the Skia context
        // must not overlap with any other submission to the queue.
        let queue = unsafe { device.queue_handle() };
        Ok(Self {
            entry: instance.entry(),
            instance: instance.ash(),
            physical: device.physical_device().handle(),
            device: device.ash(),
            queue,
            family: device.queue_family(),
            api_version: instance.api_version(),
            instance_extensions: extension_names(instance.extensions())?,
            device_extensions: extension_names(device.extensions())?,
            ycbcr_conversion,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Glyphs {
    Atlas,
    Paths,
}

impl Glyphs {
    pub fn raster_mode(self) -> RasterMode {
        match self {
            Self::Atlas => RasterMode::Vector,
            Self::Paths => RasterMode::Tiles,
        }
    }
}

pub(super) struct SkiaContext {
    context: DirectContext,
    device: Arc<Device>,
}

impl SkiaContext {
    pub fn for_device(
        device: &Arc<Device>,
        glyphs: Glyphs,
        cache_bytes: usize,
    ) -> Result<Self, Error> {
        let handles = SkiaDevice::of(device, YcbcrConversion::AsEnabledOnDevice)?;
        // SAFETY: skia-safe has no safe constructor, because Skia stores raw handles that no Rust lifetime can bind.
        // The handles are valid as long as the context: `SkiaContext` has the `Arc` of the device, which has a
        // reference to the instance, and declares that `Arc` after the Skia context, so it drops the context first. Submissions to the queue never
        // overlap: the renderer is not `Send`, and every object that submits through this device belongs to the
        // renderer on the same thread.
        let context = unsafe { direct_context(&handles, glyphs, cache_bytes) }?;
        Ok(Self {
            context,
            device: device.clone(),
        })
    }

    pub fn of_host(context: DirectContext, device: Arc<Device>) -> Self {
        Self { context, device }
    }

    pub fn device(&self) -> &Arc<Device> {
        &self.device
    }
}

/// # Safety
/// The objects of `device` must outlive the returned context. The submissions of the returned context to the queue
/// must not overlap with any other submission to the queue.
pub(super) unsafe fn direct_context(
    device: &SkiaDevice,
    glyphs: Glyphs,
    cache_bytes: usize,
) -> Result<DirectContext, Error> {
    let get_proc = |of: skvk::GetProcOf| {
        let function = match of {
            skvk::GetProcOf::Instance(handle, name) => ash_handle(handle).map(|instance| {
                // SAFETY: ash has no safe function lookup. Skia looks up functions of the instance and the device
                // given to it, which are valid by the contract of this function, with name strings that are valid for
                // the duration of the call.
                unsafe { (device.entry).get_instance_proc_addr(instance, name) }
            }),
            skvk::GetProcOf::Device(handle, name) => ash_handle(handle).map(|device_handle| {
                // SAFETY: as for the instance functions above.
                unsafe { (device.instance).get_device_proc_addr(device_handle, name) }
            }),
        };
        match function {
            Ok(Some(function)) => function as *const c_void,
            Ok(None) => std::ptr::null(),
            Err(error) => {
                log::error!("Skia asks for a function of a handle that ash cannot take: {error}");
                std::ptr::null()
            }
        }
    };
    let version = skvk::Version::new(
        skia_count(vk::api_version_major(device.api_version))?,
        skia_count(vk::api_version_minor(device.api_version))?,
        skia_count(vk::api_version_patch(device.api_version))?,
    );
    let builder = skvk::BackendContext::new_builder(
        skia_handle(device.instance.handle())?,
        skia_handle(device.physical)?,
        skia_handle(device.device.handle())?,
        (skia_handle(device.queue)?, skia_count(device.family)?),
        &get_proc,
        Some(version),
    )
    .with_extensions(&device.instance_extensions, &device.device_extensions)
    .with_sampler_ycbcr_conversion(device.ycbcr_conversion);
    let path_text_size = match glyphs {
        Glyphs::Atlas => SKIA_LARGE_DISTANCE_FIELD_FONT_LIMIT,
        Glyphs::Paths => EVERY_FONT_SIZE_AS_PATHS,
    };
    let mut options = gpu::ContextOptions::default();
    options.min_distance_field_font_size = path_text_size;
    options.glyphs_as_paths_font_size = path_text_size;
    options.disable_distance_field_paths = true;
    // SAFETY: skia-safe declares the build unsafe because the backend context contains raw handles without a
    // lifetime. The handles are valid by the contract of this function. No feature chain is borrowed, because the
    // YCbCr feature is passed by value.
    let backend = unsafe { builder.build() };
    let mut context =
        gpu::direct_contexts::make_vulkan(&backend, Some(&options)).ok_or(Error::SkiaContext)?;
    context.set_resource_cache_limit(cache_bytes);
    Ok(context)
}

fn skia_handle<T>(handle: impl Handle) -> Result<*mut T, Error> {
    let address = usize::try_from(handle.as_raw()).map_err(Error::SkiaHandoff)?;
    Ok(std::ptr::with_exposed_provenance_mut(address))
}

fn ash_handle<H: Handle, T>(pointer: *mut T) -> Result<H, Error> {
    let raw = u64::try_from(pointer.expose_provenance()).map_err(Error::SkiaHandoff)?;
    Ok(H::from_raw(raw))
}

fn skia_count(value: u32) -> Result<usize, Error> {
    usize::try_from(value).map_err(Error::SkiaHandoff)
}

pub(super) fn gpu_render_target(
    context: &mut gpu::RecordingContext,
    info: &ImageInfo,
    budgeted: gpu::Budgeted,
) -> Option<Surface> {
    gpu::surfaces::render_target(
        context,
        budgeted,
        info,
        None,
        SurfaceOrigin::TopLeft,
        None,
        false,
        false,
    )
}

pub(super) fn submit_recorded(context: &mut DirectContext) -> Result<(), Error> {
    if context.flush_and_submit().abandoned() {
        return Err(Error::SkiaAbandoned);
    }
    Ok(())
}

pub(super) fn set_texture_states<'a>(
    context: &mut DirectContext,
    textures: impl IntoIterator<Item = &'a gpu::BackendTexture>,
    state: &gpu::MutableTextureState,
    refused: Error,
) -> Result<(), Error> {
    let mut accepted = true;
    for texture in textures {
        accepted &= context.set_backend_texture_state(texture, state);
    }
    submit_recorded(context)?;
    if accepted { Ok(()) } else { Err(refused) }
}

pub(super) fn context_state(context: &mut DirectContext) -> ContextState {
    if context.is_device_lost() {
        ContextState::DeviceLost
    } else if context.oomed() {
        ContextState::OutOfMemory
    } else {
        ContextState::Usable
    }
}

fn extension_names(names: &[&'static CStr]) -> Result<Vec<&'static str>, Error> {
    (names.iter())
        .map(|name| (name.to_str()).map_err(|error| Error::ExtensionName(name, error)))
        .collect()
}

impl std::ops::Deref for SkiaContext {
    type Target = DirectContext;

    fn deref(&self) -> &DirectContext {
        &self.context
    }
}

impl std::ops::DerefMut for SkiaContext {
    fn deref_mut(&mut self) -> &mut DirectContext {
        &mut self.context
    }
}

enum_with_values! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(crate) enum SkiaFormat {
        Bgra8,
        Rgba8,
        Srgba8,
        Rgb10A2,
        Bgr10A2,
        Rgba16,
        Rgba16Float,
    }

    const ALL;
}

impl SkiaFormat {
    pub const fn vulkan(self) -> vk::Format {
        match self {
            Self::Bgra8 => vk::Format::B8G8R8A8_UNORM,
            Self::Rgba8 => vk::Format::R8G8B8A8_UNORM,
            Self::Srgba8 => vk::Format::R8G8B8A8_SRGB,
            Self::Rgb10A2 => vk::Format::A2B10G10R10_UNORM_PACK32,
            Self::Bgr10A2 => vk::Format::A2R10G10B10_UNORM_PACK32,
            Self::Rgba16 => vk::Format::R16G16B16A16_UNORM,
            Self::Rgba16Float => vk::Format::R16G16B16A16_SFLOAT,
        }
    }

    pub const fn skia(self) -> skvk::Format {
        match self {
            Self::Bgra8 => skvk::Format::B8G8R8A8_UNORM,
            Self::Rgba8 => skvk::Format::R8G8B8A8_UNORM,
            Self::Srgba8 => skvk::Format::R8G8B8A8_SRGB,
            Self::Rgb10A2 => skvk::Format::A2B10G10R10_UNORM_PACK32,
            Self::Bgr10A2 => skvk::Format::A2R10G10B10_UNORM_PACK32,
            Self::Rgba16 => skvk::Format::R16G16B16A16_UNORM,
            Self::Rgba16Float => skvk::Format::R16G16B16A16_SFLOAT,
        }
    }

    pub const fn color_type(self) -> ColorType {
        match self {
            Self::Bgra8 => ColorType::BGRA8888,
            Self::Rgba8 => ColorType::RGBA8888,
            Self::Srgba8 => ColorType::SRGBA8888,
            Self::Rgb10A2 => ColorType::RGBA1010102,
            Self::Bgr10A2 => ColorType::BGRA1010102,
            Self::Rgba16 => ColorType::R16G16B16A16UNorm,
            Self::Rgba16Float => ColorType::RGBAF16,
        }
    }

    const fn is_linear(self) -> bool {
        match self {
            Self::Srgba8 | Self::Rgba16Float => true,
            Self::Bgra8 | Self::Rgba8 | Self::Rgb10A2 | Self::Bgr10A2 | Self::Rgba16 => false,
        }
    }

    pub fn of(format: vk::Format) -> Option<Self> {
        Self::ALL.into_iter().find(|known| known.vulkan() == format)
    }

    pub fn color_space(self) -> ColorSpace {
        if self.is_linear() {
            ColorSpace::new_srgb_linear()
        } else {
            ColorSpace::new_srgb()
        }
    }
}

pub(super) fn skia_layout(layout: vk::ImageLayout) -> Option<skvk::ImageLayout> {
    Some(match layout {
        vk::ImageLayout::UNDEFINED => skvk::ImageLayout::UNDEFINED,
        vk::ImageLayout::GENERAL => skvk::ImageLayout::GENERAL,
        vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL => skvk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL => skvk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        vk::ImageLayout::TRANSFER_SRC_OPTIMAL => skvk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        vk::ImageLayout::TRANSFER_DST_OPTIMAL => skvk::ImageLayout::TRANSFER_DST_OPTIMAL,
        vk::ImageLayout::PREINITIALIZED => skvk::ImageLayout::PREINITIALIZED,
        vk::ImageLayout::READ_ONLY_OPTIMAL => skvk::ImageLayout::READ_ONLY_OPTIMAL,
        vk::ImageLayout::ATTACHMENT_OPTIMAL => skvk::ImageLayout::ATTACHMENT_OPTIMAL,
        vk::ImageLayout::PRESENT_SRC_KHR => skvk::ImageLayout::PRESENT_SRC_KHR,
        vk::ImageLayout::SHARED_PRESENT_KHR => skvk::ImageLayout::SHARED_PRESENT_KHR,
        _ => return None,
    })
}

pub(super) struct SkiaImage {
    pub image: vk::Image,
    pub layout: skvk::ImageLayout,
    pub format: skvk::Format,
    pub usage: vk::ImageUsageFlags,
    pub queue_family: u32,
    pub conversion: Option<skvk::YcbcrConversionInfo>,
}

/// # Safety
/// `image.image` must exist until Skia has finished every use of the returned info and of every Skia object made from
/// it.
pub(super) unsafe fn image_info(image: &SkiaImage) -> Result<skvk::ImageInfo, Error> {
    let handle = skia_handle(image.image)?;
    // SAFETY: skia-safe declares the constructor unsafe because the info contains the image without a lifetime. By
    // the contract of this function, the image exists as long as Skia uses it. The info has no memory allocation.
    let mut info = unsafe {
        skvk::ImageInfo::new(
            handle,
            skvk::Alloc::default(),
            skvk::ImageTiling::OPTIMAL,
            image.layout,
            image.format,
            1,
            image.queue_family,
            image.conversion,
            None,
            None,
        )
    };
    info.image_usage_flags = image.usage.as_raw();
    Ok(info)
}

/// # Safety
/// The contract of `image_info` applies.
pub(super) unsafe fn backend_texture(
    image: &SkiaImage,
    size: (i32, i32),
    label: &str,
) -> Result<gpu::BackendTexture, Error> {
    // SAFETY: `image_info` is unsafe because Skia keeps the image without a lifetime, and only the caller knows how
    // long the image exists. The caller meets the contract of `image_info`.
    let info = unsafe { image_info(image) }?;
    // SAFETY: skia-safe declares the texture unsafe because the texture contains the image of `info` without a
    // lifetime. By the contract of this function, the image exists as long as Skia uses it.
    Ok(unsafe { gpu::backend_textures::make_vk(size, &info, label) })
}

/// # Safety
/// `semaphore` must exist until every Skia submission that waits on it or signals it is complete.
pub(super) unsafe fn backend_semaphore(
    semaphore: vk::Semaphore,
) -> Result<gpu::BackendSemaphore, Error> {
    let handle = skia_handle(semaphore)?;
    // SAFETY: skia-safe declares the wrapper unsafe because the wrapper contains the semaphore without a lifetime. By
    // the contract of this function, the semaphore exists as long as Skia uses it.
    Ok(unsafe { gpu::backend_semaphores::make_vk(handle) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_mapped_format_keeps_its_value() {
        let core_formats = 0..=vk::Format::ASTC_12X12_SRGB_BLOCK.as_raw();
        let mapped = core_formats
            .filter_map(|raw| {
                SkiaFormat::of(vk::Format::from_raw(raw)).map(|known| (raw, known.skia()))
            })
            .inspect(|&(raw, skia)| assert_eq!(i64::from(raw), skia as i64))
            .count();
        assert!(mapped > 0);
    }

    #[test]
    fn every_mapped_layout_keeps_its_value() {
        let layouts = (0..=vk::ImageLayout::PREINITIALIZED.as_raw())
            .chain(1_000_000_000..=vk::ImageLayout::ATTACHMENT_OPTIMAL.as_raw());
        let mapped = layouts
            .filter_map(|raw| skia_layout(vk::ImageLayout::from_raw(raw)).map(|skia| (raw, skia)))
            .inspect(|&(raw, skia)| assert_eq!(i64::from(raw), skia as i64))
            .count();
        assert!(mapped > 0);
    }

    #[test]
    fn a_value_that_skia_does_not_draw_has_no_skia_counterpart() {
        assert_eq!(SkiaFormat::of(vk::Format::D32_SFLOAT), None);
        assert_eq!(SkiaFormat::of(vk::Format::from_raw(i32::MAX)), None);
        assert_eq!(skia_layout(vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL), None);
        assert_eq!(skia_layout(vk::ImageLayout::from_raw(i32::MAX)), None);
    }
}
