// SPDX-License-Identifier: GPL-3.0-only
use super::presentation::Presentation;
use crate::Error;
use ash::vk;
use lavashell_vulkan::{ExtensionSet, Extensions, Instance, PhysicalDevice};
use std::sync::Arc;

pub(super) struct PickedDevice<'i> {
    pub physical: PhysicalDevice<'i>,
    pub family: u32,
    pub extensions: Extensions,
}

/// The sort key by which the renderer picks one of the suitable devices, compared in this order: the rank of the
/// device type, the size of the largest device-local heap, and the device UUID. The UUID orders devices that are
/// equal in both other values the same way on every start, independent of the order in which the driver lists them.
type Precedence = (u8, Option<vk::DeviceSize>, [u8; vk::UUID_SIZE]);

/// The preference of the renderer for a device type, higher for a preferred type. From the highest to the lowest:
/// discrete GPUs, integrated GPUs, other devices, CPU implementations.
fn rank(device_type: vk::PhysicalDeviceType) -> u8 {
    match device_type {
        vk::PhysicalDeviceType::DISCRETE_GPU => 3,
        vk::PhysicalDeviceType::INTEGRATED_GPU => 2,
        vk::PhysicalDeviceType::CPU => 0,
        _ => 1,
    }
}

/// The size of the largest device-local memory heap of `physical`, `None` on a device without one.
pub(super) fn largest_device_local_heap(physical: &PhysicalDevice<'_>) -> Option<vk::DeviceSize> {
    let memory = physical.memory_properties();
    (memory.memory_heaps_as_slice().iter())
        .filter(|heap| heap.flags.contains(vk::MemoryHeapFlags::DEVICE_LOCAL))
        .map(|heap| heap.size)
        .max()
}

/// The device with the highest `Precedence` among the devices that support Vulkan 1.1 and have a graphics queue that
/// can present to `platform`. Where `platform` has a window system, the device also needs the swapchain extension.
pub(super) fn pick_device<'i>(
    instance: &'i Arc<Instance>,
    platform: &impl Presentation,
) -> Result<PickedDevice<'i>, Error> {
    let mut candidates = Vec::new();
    for physical in instance
        .physical_devices()
        .map_err(Error::PhysicalDevices)?
    {
        candidates.extend(candidate(physical, platform)?);
    }
    candidates
        .into_iter()
        .max_by_key(|(precedence, _)| *precedence)
        .map(|(_, picked)| picked)
        .ok_or(Error::NoDevice)
}

fn candidate<'i>(
    physical: PhysicalDevice<'i>,
    platform: &impl Presentation,
) -> Result<Option<(Precedence, PickedDevice<'i>)>, Error> {
    let properties = physical.properties();
    if properties.api_version < vk::API_VERSION_1_1 {
        return Ok(None);
    }
    let extensions = physical.extensions().map_err(Error::PhysicalDevice)?;
    if platform.surface_extension().is_some() && !extensions.contains(ash::khr::swapchain::NAME) {
        return Ok(None);
    }
    for (family, queues) in (0..).zip(physical.queue_families()) {
        if !queues.queue_flags.contains(vk::QueueFlags::GRAPHICS) {
            continue;
        }
        let presentation = platform
            .supports(&physical, family)
            .map_err(Error::PhysicalDevice)?;
        if presentation == lavashell_vulkan::Presentation::Supported {
            let uuid = physical.device_uuid().map_err(Error::PhysicalDevice)?;
            let precedence = (
                rank(properties.device_type),
                largest_device_local_heap(&physical),
                uuid,
            );
            let picked = PickedDevice {
                physical,
                family,
                extensions,
            };
            return Ok(Some((precedence, picked)));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpus_come_before_other_devices_and_cpu_implementations_last() {
        let ranks = [
            vk::PhysicalDeviceType::DISCRETE_GPU,
            vk::PhysicalDeviceType::INTEGRATED_GPU,
            vk::PhysicalDeviceType::VIRTUAL_GPU,
            vk::PhysicalDeviceType::CPU,
        ]
        .map(rank);
        assert!(ranks.is_sorted_by(|preferred, next| preferred > next));
    }
}
