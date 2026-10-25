// SPDX-License-Identifier: GPL-3.0-only
//! The Vulkan handles behind the wgpu objects of Bevy. wgpu 26 and Bevy 0.17 return those handles only through unsafe
//! calls. This module contains every call of this crate into wgpu-hal.

use crate::Error;
use crate::texture_types::{hal_uses, image_description};
use ash::vk;
use bevy::prelude::{Resource, World};
use bevy::render::render_resource::Texture;
use bevy::render::renderer::raw_vulkan_init::RawVulkanInitSettings;
use bevy::render::renderer::{RenderDevice, RenderQueue};
use lavashell_vulkan::{Device, ExportableImage, Features, HostDevice};
use std::sync::Arc;
use wgpu::hal::api::Vulkan;

/// The Vulkan device of Bevy, wrapped as a lavashell-vulkan device. The render world contains one, so the overlay and
/// the scene export submit to the queue under the same lock.
#[derive(Resource)]
struct SharedDevice(Arc<Device>);

/// The lavashell-vulkan device that wraps the Vulkan device behind the render device of `world`. The first call creates
/// it and inserts it into `world` as a resource. Later calls return the same device.
///
/// # Errors
/// Fails when the render device is not a Vulkan device.
///
/// # Safety
/// Every submission of Bevy to its queue must be serialized with the submissions made through the returned device,
/// for the lifetime of the app.
pub(crate) unsafe fn shared_device(world: &mut World) -> Result<Arc<Device>, Error> {
    if let Some(SharedDevice(device)) = world.get_resource::<SharedDevice>() {
        return Ok(device.clone());
    }
    let host = host_device(world.resource::<RenderDevice>())?;
    // SAFETY: lavashell-vulkan cannot check handles that it did not create. It wraps the device of a host engine only
    // through the unsafe `Device::borrow`, which has no safe form. The handles come from the wgpu device of Bevy. The
    // clone of that wgpu device in `host.keep_alive` prevents wgpu from destroying them. They are the instance,
    // device, queue and enabled extensions of that wgpu device, and `host.features` claims no optional feature. The
    // contract of this function serializes the submissions made by Bevy with the submissions made through the
    // borrowed device.
    let device = unsafe { Device::borrow(host) };
    world.insert_resource(SharedDevice(device.clone()));
    Ok(device)
}

/// The handles of the Vulkan device behind `render`. They are valid while the clone of the wgpu device of `render` in
/// `keep_alive` exists.
fn host_device(render: &RenderDevice) -> Result<HostDevice, Error> {
    let owner = render.wgpu_device().clone();
    // SAFETY: wgpu returns the Vulkan device of a wgpu device only through `as_hal`, which is unsafe. Bevy has no other
    // accessor for it. The condition of `as_hal` is that nothing destroys the
    // objects returned through the guard. This function never destroys the handles copied from the guard, and the
    // wgpu device exists while `owner` or its clone in `keep_alive` exists.
    let device = unsafe { owner.as_hal::<Vulkan>() }.ok_or(Error::NotVulkanDevice)?;
    let instance = device.shared_instance();
    let host = HostDevice {
        entry: instance.entry().clone(),
        instance: instance.raw_instance().clone(),
        instance_api_version: instance.instance_api_version(),
        instance_extensions: instance.extensions().to_vec(),
        device: device.raw_device().clone(),
        physical: device.raw_physical_device(),
        queue: device.raw_queue(),
        queue_family: device.queue_family_index(),
        device_extensions: device.enabled_device_extensions().to_vec(),
        // `Features::default()` claims no optional feature, which is correct for every device that wgpu creates.
        features: Features::default(),
        keep_alive: Box::new(owner.clone()),
    };
    drop(device);
    Ok(host)
}

/// Makes the next submission of wgpu on `queue` signal `semaphore`.
pub(crate) fn signal_on_queue(queue: &RenderQueue, semaphore: vk::Semaphore) -> Result<(), Error> {
    // SAFETY: wgpu 26 returns the Vulkan queue of a wgpu queue only through `as_hal`, which is unsafe. This function
    // uses the guard only to add a signal semaphore and destroys nothing through it. The guard is dropped when this
    // function returns, before the next call into wgpu.
    let queue = unsafe { queue.as_hal::<Vulkan>() }.ok_or(Error::NotVulkanQueue)?;
    queue.add_signal_semaphore(semaphore, None);
    Ok(())
}

/// The Vulkan image behind `texture`.
pub(crate) fn image_of(texture: &wgpu::Texture) -> Result<vk::Image, Error> {
    // SAFETY: wgpu 26 returns the wgpu-hal texture of a wgpu texture only through `as_hal`, which is unsafe. This
    // function only reads through the guard. The guard is dropped when this function returns.
    let texture = unsafe { texture.as_hal::<Vulkan>() }.ok_or(Error::NotVulkanTexture)?;
    // SAFETY: wgpu-hal 26 returns the Vulkan image of a texture only through `raw_handle`, which is unsafe. The
    // condition of `raw_handle` is that no code outside wgpu destroys the image. The callers of this function only
    // draw into the image.
    Ok(unsafe { texture.raw_handle() })
}

/// A wgpu texture of `format` and `usage` whose storage is the exportable Vulkan image `image`. The texture contains an
/// `Arc` of the image, so the image exists until wgpu drops the texture.
///
/// # Errors
/// Fails when `format` and `usage` do not describe `image`, or when `render` is not a Vulkan device.
///
/// # Safety
/// The contents of `image` must be initialized: the commands that wrote them must be complete.
pub(crate) unsafe fn wrap_external_image(
    render: &RenderDevice,
    image: &ExportableImage,
    format: wgpu::TextureFormat,
    usage: wgpu::TextureUsages,
) -> Result<Texture, Error> {
    let described = image.description();
    if image_description(format, usage, described.extent)? != described {
        return Err(Error::TextureDescription);
    }
    let size = wgpu::Extent3d {
        width: described.extent.width,
        height: described.extent.height,
        depth_or_array_layers: 1,
    };
    let label = Some("exported scene target");
    let descriptor = wgpu::TextureDescriptor {
        label,
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage,
        view_formats: &[],
    };
    let hal_descriptor = wgpu::hal::TextureDescriptor {
        label,
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: hal_uses(usage)?,
        memory_flags: wgpu::hal::MemoryFlags::empty(),
        view_formats: Vec::new(),
    };
    let keep_alive = image.image().clone();
    // SAFETY: wgpu 26 returns the wgpu-hal device of a wgpu device only through `as_hal`, which is unsafe. This
    // function uses the guard only to wrap an image and destroys nothing through it. The guard is dropped before the
    // next call into wgpu.
    let device =
        unsafe { render.wgpu_device().as_hal::<Vulkan>() }.ok_or(Error::NotVulkanDevice)?;
    // SAFETY: wgpu-hal 26 wraps an image created outside wgpu only through `texture_from_raw`, which is unsafe. The
    // image is a single-sample 2D image with one mip level and one layer. Its format, extent and usage match
    // `hal_descriptor`, as checked at the start of this function. The image has no mutable format, and
    // `hal_descriptor` lists no view formats. The drop callback contains an `Arc` of the image until wgpu-hal drops
    // the texture.
    let hal = unsafe {
        device.texture_from_raw(
            image.image().handle(),
            &hal_descriptor,
            Some(Box::new(move || drop(keep_alive))),
        )
    };
    drop(device);
    // SAFETY: wgpu 26 wraps a wgpu-hal texture only through `create_texture_from_hal`, which is unsafe. The wgpu-hal
    // texture was created through the device of `render`. `descriptor` describes the same texture as
    // `hal_descriptor`. The contract of this function requires the caller to initialize the contents of the image.
    let texture = unsafe {
        render
            .wgpu_device()
            .create_texture_from_hal::<Vulkan>(hal, &descriptor)
    };
    Ok(texture.into())
}

/// Adds `VK_KHR_external_memory_fd` to the extensions of the Vulkan device created by Bevy, when the adapter supports
/// it. Scene export needs the extension to export the memory of a scene image as a file descriptor.
pub(crate) fn request_external_memory_fd(settings: &mut RawVulkanInitSettings) {
    let name = ash::khr::external_memory_fd::NAME;
    // SAFETY: bevy_render 0.17 registers a device callback only through `add_create_device_callback`, which is
    // unsafe. Its conditions are that the callback removes no feature and requests nothing that the device does not
    // support. The callback only adds one extension, and only when the adapter reports support for it.
    unsafe {
        settings.add_create_device_callback(move |arguments, adapter, _| {
            if !adapter
                .physical_device_capabilities()
                .supports_extension(name)
            {
                log::error!(
                    "the Vulkan device has no VK_KHR_external_memory_fd, so scenes cannot be exported"
                );
            } else if !arguments.extensions.contains(&name) {
                arguments.extensions.push(name);
            }
        });
    }
}
