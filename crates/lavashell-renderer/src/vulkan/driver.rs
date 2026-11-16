// SPDX-License-Identifier: GPL-3.0-only
use super::PresentationBackend;
use crate::Error;
use ash::vk;
use lavashell_vulkan::{ExtensionSet, Extensions, Instance, PhysicalDevice, Swapchain};
use std::sync::Arc;

pub(super) struct PickedDevice<'i> {
    pub physical: PhysicalDevice<'i>,
    pub family: u32,
    pub extensions: Extensions,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct DevicePrecedence {
    rank: u8,
    largest_heap: Option<vk::DeviceSize>,
    uuid: [u8; vk::UUID_SIZE],
}

fn rank(device_type: vk::PhysicalDeviceType) -> u8 {
    match device_type {
        vk::PhysicalDeviceType::DISCRETE_GPU => 3,
        vk::PhysicalDeviceType::INTEGRATED_GPU => 2,
        vk::PhysicalDeviceType::CPU => 0,
        _ => 1,
    }
}

pub(super) fn largest_device_local_heap(physical: &PhysicalDevice<'_>) -> Option<vk::DeviceSize> {
    let memory = physical.memory_properties();
    (memory.memory_heaps_as_slice().iter())
        .filter(|heap| heap.flags.contains(vk::MemoryHeapFlags::DEVICE_LOCAL))
        .map(|heap| heap.size)
        .max()
}

pub(super) fn pick_device<'i>(
    instance: &'i Arc<Instance>,
    backend: &impl PresentationBackend,
) -> Result<PickedDevice<'i>, Error> {
    let mut candidates = Vec::new();
    for physical in instance
        .physical_devices()
        .map_err(Error::PhysicalDevices)?
    {
        candidates.extend(candidate(physical, backend)?);
    }
    candidates
        .into_iter()
        .max_by_key(|(precedence, _)| *precedence)
        .map(|(_, picked)| picked)
        .ok_or(Error::NoDevice)
}

fn candidate<'i>(
    physical: PhysicalDevice<'i>,
    backend: &impl PresentationBackend,
) -> Result<Option<(DevicePrecedence, PickedDevice<'i>)>, Error> {
    let properties = physical.properties();
    if properties.api_version < vk::API_VERSION_1_1 {
        return Ok(None);
    }
    let extensions = physical.extensions().map_err(Error::PhysicalDevice)?;
    if backend.surface_extension().is_some() {
        let features = physical.features().map_err(Error::PhysicalDevice)?;
        let swapchain = extensions.require(&Swapchain::device_extensions());
        if swapchain.is_err() || !features.swapchain_maintenance1 {
            return Ok(None);
        }
    }
    for (family, queues) in (0..).zip(physical.queue_families()) {
        if !queues.queue_flags.contains(vk::QueueFlags::GRAPHICS) {
            continue;
        }
        let presentation = backend
            .supports(&physical, family)
            .map_err(Error::PhysicalDevice)?;
        if presentation == lavashell_vulkan::Presentation::Supported {
            let uuid = physical.device_uuid().map_err(Error::PhysicalDevice)?;
            let precedence = DevicePrecedence {
                rank: rank(properties.device_type),
                largest_heap: largest_device_local_heap(&physical),
                uuid,
            };
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
