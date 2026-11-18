// SPDX-License-Identifier: GPL-3.0-only

use super::driver::largest_device_local_heap;
use super::skia_handoff::SkiaContext;
use crate::Error;
use ash::vk;
use lavashell_vulkan::PhysicalDevice;

fn limit(physical: &PhysicalDevice<'_>, held: usize) -> Result<usize, Error> {
    let bytes = match physical.memory_budget().map_err(Error::PhysicalDevice)? {
        Some(heaps) => (heaps.iter())
            .filter(|heap| heap.heap.flags.contains(vk::MemoryHeapFlags::DEVICE_LOCAL))
            .map(|heap| addressable(heap.budget.saturating_sub(heap.usage)).saturating_add(held))
            .max(),
        None => largest_device_local_heap(physical).map(addressable),
    };
    bytes.ok_or(Error::NoDeviceLocalHeap)
}

pub(super) fn initial_limit(physical: &PhysicalDevice<'_>) -> Result<usize, Error> {
    limit(physical, 0)
}

pub(super) fn refresh(context: &mut SkiaContext) -> Result<(), Error> {
    let held = context.resource_cache_usage().resource_bytes;
    let limit = limit(&context.device().physical_device(), held)?;
    context.set_resource_cache_limit(limit);
    Ok(())
}

fn addressable(bytes: vk::DeviceSize) -> usize {
    usize::try_from(bytes).unwrap_or(usize::MAX)
}
