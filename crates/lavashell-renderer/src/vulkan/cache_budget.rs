// SPDX-License-Identifier: GPL-3.0-only
//! The byte limit of the Skia resource cache. The limit is computed from the memory of the GPU device that is free at
//! the moment of each refresh.

use super::driver::largest_device_local_heap;
use crate::Error;
use ash::vk;
use lavashell_vulkan::{Device, PhysicalDevice};
use skia_safe::gpu::DirectContext;

/// The number of bytes that the Skia resource cache may use on `physical` while it already uses `held` bytes. On a
/// device that reports its memory budget, the limit is `held` plus the largest budget left in a device-local heap
/// after all current use of that heap. On any other device, the limit is the size of the largest device-local heap.
pub(super) fn limit(physical: &PhysicalDevice<'_>, held: usize) -> Result<usize, Error> {
    let bytes = match physical.memory_budget().map_err(Error::PhysicalDevice)? {
        Some(heaps) => (heaps.iter())
            .filter(|heap| heap.heap.flags.contains(vk::MemoryHeapFlags::DEVICE_LOCAL))
            .map(|heap| addressable(heap.budget.saturating_sub(heap.usage)).saturating_add(held))
            .max(),
        None => largest_device_local_heap(physical).map(addressable),
    };
    bytes.ok_or(Error::NoDeviceLocalHeap)
}

/// Sets the limit of the Skia resource cache of `context` from the memory of `device` that is free at this moment.
pub(super) fn refresh(context: &mut DirectContext, device: &Device) -> Result<(), Error> {
    let held = context.resource_cache_usage().resource_bytes;
    context.set_resource_cache_limit(limit(&device.physical_device(), held)?);
    Ok(())
}

/// `bytes` as the `size_t` in which Skia counts the size of its cache. A count beyond the address space of the process
/// becomes `usize::MAX`.
fn addressable(bytes: vk::DeviceSize) -> usize {
    usize::try_from(bytes).unwrap_or(usize::MAX)
}
