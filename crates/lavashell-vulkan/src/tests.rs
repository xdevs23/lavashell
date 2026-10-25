// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use ash::vk;
use std::num::TryFromIntError;
use std::sync::Arc;

const EXTERNAL: [&std::ffi::CStr; 2] = [
    ash::khr::external_memory_fd::NAME,
    ash::khr::external_semaphore_fd::NAME,
];

#[derive(Debug, thiserror::Error)]
enum TestError {
    #[error(transparent)]
    Vulkan(#[from] Error),
    #[error("the machine offers no Vulkan physical device")]
    NoPhysicalDevice,
    #[error("the physical device has no graphics queue family")]
    NoGraphicsQueueFamily,
    #[error("the index of the graphics queue family does not fit 32 bits")]
    QueueFamilyIndex(#[source] TryFromIntError),
    #[error("the number of memory heaps does not fit the size of a list")]
    HeapCount(#[source] TryFromIntError),
}

fn device() -> Result<Arc<Device>, TestError> {
    let instance = Instance::create(&InstanceRequest {
        api_version: vk::API_VERSION_1_1,
        extensions: &[],
    })?;
    let physical = instance
        .physical_devices()?
        .into_iter()
        .next()
        .ok_or(TestError::NoPhysicalDevice)?;
    let family = (physical.queue_families().iter())
        .position(|family| family.queue_flags.contains(vk::QueueFlags::GRAPHICS))
        .ok_or(TestError::NoGraphicsQueueFamily)?;
    let device = Device::create(
        physical,
        &DeviceRequest {
            queue_family: u32::try_from(family).map_err(TestError::QueueFamilyIndex)?,
            extensions: &EXTERNAL,
            features: Features::default(),
        },
    )?;
    Ok(device)
}

fn description() -> ImageDescription {
    ImageDescription {
        format: vk::Format::R8G8B8A8_SRGB,
        extent: vk::Extent2D {
            width: 8,
            height: 8,
        },
        usage: vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST,
    }
}

#[test]
fn a_physical_device_describes_itself() -> Result<(), TestError> {
    let device = device()?;
    let physical = device.physical_device();
    assert_ne!(physical.name()?, "");
    assert!(
        physical
            .extensions()?
            .contains(ash::khr::external_memory_fd::NAME)
    );
    assert_ne!(physical.device_uuid()?, [0; vk::UUID_SIZE]);
    assert!(physical.memory_properties().memory_type_count > 0);
    Ok(())
}

#[test]
fn a_device_without_a_requested_extension_is_refused() -> Result<(), TestError> {
    let instance = Instance::create(&InstanceRequest {
        api_version: vk::API_VERSION_1_1,
        extensions: &[],
    })?;
    let physical = instance
        .physical_devices()?
        .into_iter()
        .next()
        .ok_or(TestError::NoPhysicalDevice)?;
    let created = Device::create(
        physical,
        &DeviceRequest {
            queue_family: 0,
            extensions: &[c"VK_LAVASHELL_absent"],
            features: Features::default(),
        },
    );
    assert!(
        matches!(created, Err(Error::MissingExtension(name)) if name == c"VK_LAVASHELL_absent")
    );
    Ok(())
}

#[test]
fn a_cleared_image_is_released_and_imported_by_another_device() -> Result<(), TestError> {
    let producer = device()?;
    let consumer = device()?;
    let exportable = Image::exportable(&producer, &description())?;
    let image = exportable.image();
    let mut recording = Commands::new(&producer)?.record()?;
    recording.discard(image, vk::ImageLayout::TRANSFER_DST_OPTIMAL)?;
    let red = vk::ClearColorValue {
        float32: [1.0, 0.0, 0.0, 1.0],
    };
    recording.clear(image, red)?;
    let released = ImageState::new(vk::ImageLayout::GENERAL, QueueFamily::External);
    recording.transition(image, released)?;
    let commands = recording.finish()?.submit()?.reclaim()?;
    assert!(Arc::ptr_eq(commands.device(), &producer));
    let export = exportable.export()?;
    assert_eq!(export.description(), description());
    assert_eq!(export.allocation_bytes(), image.allocation_bytes());
    let imported = Image::import(&consumer, &export.try_clone()?)?;
    assert_eq!(imported.allocation_bytes(), export.allocation_bytes());
    let mut recording = Commands::new(&consumer)?.record()?;
    // SAFETY: no check can confirm the state of an image released by another device. The release recorded on
    // `producer` above is complete: it moved the image in GENERAL to the external queue family and made its writes
    // available.
    unsafe { recording.assume(&imported, released) }?;
    let sampled = ImageState::new(
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        QueueFamily::Device,
    );
    recording.transition(&imported, sampled)?;
    let commands = recording.finish()?.submit()?.reclaim()?;
    assert!(Arc::ptr_eq(commands.device(), &consumer));
    Ok(())
}

#[test]
fn a_recording_refuses_images_it_knows_nothing_about() -> Result<(), TestError> {
    let device = device()?;
    let other = self::device()?;
    let image = Image::new(&device, &description())?;
    let mut recording = Commands::new(&device)?.record()?;
    let sampled = ImageState::new(
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        QueueFamily::Device,
    );
    assert!(matches!(
        recording.transition(&image, sampled),
        Err(Error::UnknownImageState)
    ));
    recording.discard(&image, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)?;
    let red = vk::ClearColorValue {
        float32: [1.0, 0.0, 0.0, 1.0],
    };
    assert!(matches!(
        recording.clear(&image, red),
        Err(Error::ClearLayout(
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
        ))
    ));
    let foreign = Image::new(&other, &description())?;
    assert!(matches!(
        recording.discard(&foreign, vk::ImageLayout::GENERAL),
        Err(Error::ForeignObject)
    ));
    Ok(())
}

#[test]
fn a_timer_reads_the_time_between_its_timestamps() -> Result<(), TestError> {
    let device = device()?;
    let timer = GpuTimer::new(&device)?.start()?.stop()?;
    device.signal()?.wait()?;
    assert!(matches!(timer.reading()?, Reading::Elapsed(_)));
    let timer = timer.reset()?.start()?.stop()?;
    device.signal()?.wait()?;
    assert!(matches!(timer.reading()?, Reading::Elapsed(_)));
    Ok(())
}

#[test]
fn a_signal_completes_after_the_work_before_it() -> Result<(), TestError> {
    let device = device()?;
    let signal = device.signal()?;
    signal.wait()?;
    assert_eq!(signal.status()?, Status::Complete);
    let exported = device.signal_sync_fd()?;
    exported.submission.wait()?;
    assert_eq!(exported.submission.status()?, Status::Complete);
    Ok(())
}

#[test]
fn a_device_reports_a_budget_per_heap_only_with_the_budget_extension() -> Result<(), TestError> {
    let device = device()?;
    let physical = device.physical_device();
    let offered = physical
        .extensions()?
        .contains(ash::ext::memory_budget::NAME);
    let heap_count = usize::try_from(physical.memory_properties().memory_heap_count)
        .map_err(TestError::HeapCount)?;
    let budget = physical.memory_budget()?;
    assert_eq!(
        budget.map(|heaps| heaps.len()),
        offered.then_some(heap_count)
    );
    Ok(())
}

#[test]
fn a_semaphore_of_another_device_is_not_waited_for() -> Result<(), TestError> {
    let device = device()?;
    let other = self::device()?;
    let foreign = Semaphore::new(&other)?;
    // SAFETY: the semaphore belongs to another device. The call fails before it submits anything, so no signal is
    // consumed.
    let waited = unsafe { device.wait_semaphore(&foreign) };
    assert!(matches!(waited, Err(Error::ForeignObject)));
    Ok(())
}
