// SPDX-License-Identifier: GPL-3.0-only
//! The functions that pass raw Vulkan handles to Skia. Skia stores those handles without a lifetime, so each caller
//! must ensure that the Vulkan objects are valid while Skia uses them.

use crate::{ContextState, Error};
use ash::vk::{self, Handle};
use lavashell_vulkan::Device;
use skia_safe::gpu::{self, DirectContext, vk as skvk};
use skia_safe::{ColorSpace, ColorType};
use std::ffi::{CStr, c_void};
use std::sync::Arc;

/// The device text size from which Skia draws glyphs from distance fields instead of masks: `kLargeDFFontLimit` in
/// <https://skia.googlesource.com/skia/+/refs/heads/main/src/text/gpu/SubRunControl.cpp>, the same in the Skia
/// of skia-bindings 0.153.3.
const SKIA_DISTANCE_FIELD_TEXT: f32 = 162.0;

/// The handles of a Vulkan device that a Skia context uses.
pub(super) struct SkiaDevice<'a> {
    pub entry: &'a ash::Entry,
    pub instance: &'a ash::Instance,
    pub physical: vk::PhysicalDevice,
    pub device: &'a ash::Device,
    pub queue: vk::Queue,
    pub family: u32,
    pub api_version: u32,
    pub instance_extensions: &'a [&'a str],
    pub device_extensions: &'a [&'a str],
    pub ycbcr_conversion: bool,
}

/// How Skia draws glyphs: from its alpha-mask atlas, or as paths on a device that corrupts mask coverage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Glyphs {
    Atlas,
    Paths,
}

/// A Skia context, with a reference to the device that the renderer created for it, or without one for a context on
/// the device of a host engine. The context is destroyed before that device.
pub(super) struct SkiaContext {
    context: DirectContext,
    _device: Option<Arc<Device>>,
}

impl SkiaContext {
    /// A context on a device that the renderer created.
    pub fn for_device(
        device: &Arc<Device>,
        glyphs: Glyphs,
        cache_bytes: usize,
    ) -> Result<Self, Error> {
        let instance = device.instance();
        let handles = SkiaDevice {
            entry: instance.entry(),
            instance: instance.ash(),
            physical: device.physical_device().handle(),
            device: device.ash(),
            queue: device.queue_handle(),
            family: device.queue_family(),
            api_version: instance.api_version(),
            instance_extensions: &extension_names(instance.extensions())?,
            device_extensions: &extension_names(device.extensions())?,
            ycbcr_conversion: device.features().sampler_ycbcr_conversion,
        };
        // SAFETY: skia-safe has no safe constructor, because Skia stores raw handles that no Rust lifetime can bind.
        // The handles are valid as long as the context: `SkiaContext` has the `Arc` of the device, which has a
        // reference to the instance, and drops that `Arc` only after the Skia context. Submissions to the queue never
        // overlap: the renderer is not `Send`, and every object that submits through this device belongs to the
        // renderer on the same thread.
        let context = unsafe { direct_context(&handles, glyphs, cache_bytes) }?;
        Ok(Self {
            context,
            _device: Some(device.clone()),
        })
    }
}

impl SkiaContext {
    /// A context on the handles of a host engine. The host engine is responsible for keeping those handles valid
    /// while the context exists.
    pub fn of_host(context: DirectContext) -> Self {
        Self {
            context,
            _device: None,
        }
    }
}

/// A Skia context on the handles of `device`.
///
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
            // Skia treats a null function as a function missing from the driver and then creates no context.
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
    // Skia accepts a swapchain image that is acquired again in PRESENT_SRC only when it receives the enabled
    // extensions.
    .with_extensions(device.instance_extensions, device.device_extensions)
    .with_sampler_ycbcr_conversion(device.ycbcr_conversion);
    // Skia draws text at `glyphs_as_paths_font_size` and larger as paths, and requires `min_distance_field_font_size`
    // to be positive and at most that size. With both set to the same size, no text is drawn from distance fields,
    // whose edges depend on where earlier draws placed the glyphs in the shared atlas.
    let path_text_size = match glyphs {
        Glyphs::Atlas => SKIA_DISTANCE_FIELD_TEXT,
        Glyphs::Paths => f32::MIN_POSITIVE,
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

/// A Vulkan handle as the pointer to an opaque type, the form in which Skia expects handles.
fn skia_handle<T>(handle: impl Handle) -> Result<*mut T, Error> {
    let address = usize::try_from(handle.as_raw()).map_err(Error::SkiaHandoff)?;
    Ok(std::ptr::with_exposed_provenance_mut(address))
}

/// The ash handle for a handle that Skia passes back as a pointer.
fn ash_handle<H: Handle, T>(pointer: *mut T) -> Result<H, Error> {
    let raw = u64::try_from(pointer.expose_provenance()).map_err(Error::SkiaHandoff)?;
    Ok(H::from_raw(raw))
}

/// A version number or a queue family index as the `size_t` expected by Skia.
fn skia_count(value: u32) -> Result<usize, Error> {
    usize::try_from(value).map_err(Error::SkiaHandoff)
}

/// Flushes and submits the work recorded on `context`.
///
/// Fails when Skia has abandoned the context after the loss of the device. Skia then submitted nothing.
pub(super) fn submit_recorded(context: &mut DirectContext) -> Result<(), Error> {
    if context.flush_and_submit().abandoned() {
        return Err(Error::SkiaAbandoned);
    }
    Ok(())
}

/// The state of `context`. A context that lost its device and also ran out of memory returns the lost device.
pub(super) fn context_state(context: &mut DirectContext) -> ContextState {
    if context.is_device_lost() {
        ContextState::DeviceLost
    } else if context.oomed() {
        ContextState::OutOfMemory
    } else {
        ContextState::Usable
    }
}

pub(super) fn extension_names(names: &[&'static CStr]) -> Result<Vec<&'static str>, Error> {
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

/// A color format that the renderer draws into or samples, with the matching Skia format and color type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SkiaFormat {
    pub vulkan: vk::Format,
    pub skia: skvk::Format,
    pub color_type: ColorType,
    /// Whether Skia writes linear values into the format. The format then stores the values linearly, as a float
    /// format does, or encodes them as sRGB, as an sRGB format does.
    pub linear: bool,
}

impl SkiaFormat {
    pub const BGRA8: Self = Self {
        vulkan: vk::Format::B8G8R8A8_UNORM,
        skia: skvk::Format::B8G8R8A8_UNORM,
        color_type: ColorType::BGRA8888,
        linear: false,
    };
    pub const RGBA8: Self = Self {
        vulkan: vk::Format::R8G8B8A8_UNORM,
        skia: skvk::Format::R8G8B8A8_UNORM,
        color_type: ColorType::RGBA8888,
        linear: false,
    };
    pub const SRGBA8: Self = Self {
        vulkan: vk::Format::R8G8B8A8_SRGB,
        skia: skvk::Format::R8G8B8A8_SRGB,
        color_type: ColorType::SRGBA8888,
        linear: true,
    };
    pub const RGB10_A2: Self = Self {
        vulkan: vk::Format::A2B10G10R10_UNORM_PACK32,
        skia: skvk::Format::A2B10G10R10_UNORM_PACK32,
        color_type: ColorType::RGBA1010102,
        linear: false,
    };
    pub const BGR10_A2: Self = Self {
        vulkan: vk::Format::A2R10G10B10_UNORM_PACK32,
        skia: skvk::Format::A2R10G10B10_UNORM_PACK32,
        color_type: ColorType::BGRA1010102,
        linear: false,
    };
    pub const RGBA16: Self = Self {
        vulkan: vk::Format::R16G16B16A16_UNORM,
        skia: skvk::Format::R16G16B16A16_UNORM,
        color_type: ColorType::R16G16B16A16UNorm,
        linear: false,
    };
    pub const RGBA16_FLOAT: Self = Self {
        vulkan: vk::Format::R16G16B16A16_SFLOAT,
        skia: skvk::Format::R16G16B16A16_SFLOAT,
        color_type: ColorType::RGBAF16,
        linear: true,
    };

    const ALL: [Self; 7] = [
        Self::BGRA8,
        Self::RGBA8,
        Self::SRGBA8,
        Self::RGB10_A2,
        Self::BGR10_A2,
        Self::RGBA16,
        Self::RGBA16_FLOAT,
    ];

    /// The known format whose Vulkan format is `format`, or `None` for a format that the renderer neither draws into
    /// nor samples.
    pub fn of(format: vk::Format) -> Option<Self> {
        Self::ALL.into_iter().find(|known| known.vulkan == format)
    }

    /// The color space in which Skia writes values into the format, with sRGB primaries.
    pub fn color_space(self) -> ColorSpace {
        if self.linear {
            ColorSpace::new_srgb_linear()
        } else {
            ColorSpace::new_srgb()
        }
    }
}

/// The Skia layout for `layout`, or `None` for a layout that a color image cannot be in.
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

/// A single-sample 2D color image with one mip level, optimal tiling and exclusive sharing, in the form that Skia
/// expects.
pub(super) struct SkiaImage {
    pub image: vk::Image,
    pub layout: skvk::ImageLayout,
    pub format: skvk::Format,
    pub usage: vk::ImageUsageFlags,
    pub queue_family: u32,
    pub conversion: Option<skvk::YcbcrConversionInfo>,
}

/// The Skia image info for `image`.
///
/// # Errors
/// Fails when the image handle does not fit into the pointer that Skia expects.
///
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

/// # Errors
/// Fails as `image_info` does.
///
/// # Safety
/// The contract of `image_info` applies.
pub(super) unsafe fn backend_texture(
    image: &SkiaImage,
    size: (i32, i32),
    label: &str,
) -> Result<gpu::BackendTexture, Error> {
    // SAFETY: the caller meets the contract of `image_info`.
    let info = unsafe { image_info(image) }?;
    // SAFETY: skia-safe declares the texture unsafe because the texture contains the image of `info` without a
    // lifetime. By the contract of this function, the image exists as long as Skia uses it.
    Ok(unsafe { gpu::backend_textures::make_vk(size, &info, label) })
}

/// # Errors
/// Fails when the semaphore handle does not fit into the pointer that Skia expects.
///
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
                SkiaFormat::of(vk::Format::from_raw(raw)).map(|known| (raw, known.skia))
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
