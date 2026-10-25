// SPDX-License-Identifier: GPL-3.0-only
use crate::Error;
use ash::vk;

/// The queue family that owns an image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueueFamily {
    /// The queue family of the one queue used by [`crate::Device`].
    Device,
    /// A queue of another device or API. Ownership passes to and from it through the family index
    /// `VK_QUEUE_FAMILY_EXTERNAL`.
    External,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageState {
    pub layout: vk::ImageLayout,
    pub family: QueueFamily,
}

impl ImageState {
    #[must_use]
    pub const fn new(layout: vk::ImageLayout, family: QueueFamily) -> Self {
        Self { layout, family }
    }
}

/// Returns the barriers that move an image from the state `from` into the state `to`, where `family` is the index of
/// the queue family used by the device. The image changes queue family in the layout it has on the external side. A
/// release first changes the layout and then hands the image over. An acquire first takes the image over and then
/// changes the layout. Every barrier makes all memory writes before it visible to all memory accesses after it.
pub(super) fn barriers(
    from: ImageState,
    to: ImageState,
    family: u32,
) -> Result<Vec<vk::ImageMemoryBarrier<'static>>, Error> {
    if [vk::ImageLayout::UNDEFINED, vk::ImageLayout::PREINITIALIZED].contains(&to.layout)
        && from != to
    {
        return Err(Error::TargetLayout(to.layout));
    }
    let layout = (from.layout != to.layout).then(|| {
        barrier(from.layout, to.layout)
            .src_access_mask(vk::AccessFlags::MEMORY_WRITE)
            .dst_access_mask(vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE)
    });
    let barriers = match (from.family, to.family) {
        (QueueFamily::Device, QueueFamily::Device) => layout.into_iter().collect(),
        (QueueFamily::External, QueueFamily::External) if layout.is_none() => Vec::new(),
        (QueueFamily::External, QueueFamily::External) => return Err(Error::ExternalTransition),
        (QueueFamily::Device, QueueFamily::External) => {
            let release = barrier(to.layout, to.layout)
                .src_queue_family_index(family)
                .dst_queue_family_index(vk::QUEUE_FAMILY_EXTERNAL)
                .src_access_mask(vk::AccessFlags::MEMORY_WRITE);
            layout.into_iter().chain([release]).collect()
        }
        (QueueFamily::External, QueueFamily::Device) => {
            let acquire = barrier(from.layout, from.layout)
                .src_queue_family_index(vk::QUEUE_FAMILY_EXTERNAL)
                .dst_queue_family_index(family)
                .dst_access_mask(vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE);
            [acquire].into_iter().chain(layout).collect()
        }
    };
    Ok(barriers)
}

fn barrier(old: vk::ImageLayout, new: vk::ImageLayout) -> vk::ImageMemoryBarrier<'static> {
    vk::ImageMemoryBarrier::default()
        .old_layout(old)
        .new_layout(new)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FAMILY: u32 = 3;

    fn state(layout: vk::ImageLayout, family: QueueFamily) -> ImageState {
        ImageState::new(layout, family)
    }

    fn summary(
        barriers: &[vk::ImageMemoryBarrier],
    ) -> Vec<(vk::ImageLayout, vk::ImageLayout, u32, u32)> {
        (barriers.iter())
            .map(|b| {
                (
                    b.old_layout,
                    b.new_layout,
                    b.src_queue_family_index,
                    b.dst_queue_family_index,
                )
            })
            .collect()
    }

    #[test]
    fn an_unchanged_state_needs_no_barrier() -> Result<(), Error> {
        let general = state(vk::ImageLayout::GENERAL, QueueFamily::Device);
        assert!(barriers(general, general, FAMILY)?.is_empty());
        Ok(())
    }

    #[test]
    fn a_layout_change_on_the_queue_keeps_the_owner() -> Result<(), Error> {
        let from = state(vk::ImageLayout::UNDEFINED, QueueFamily::Device);
        let to = state(vk::ImageLayout::TRANSFER_DST_OPTIMAL, QueueFamily::Device);
        let ignored = vk::QUEUE_FAMILY_IGNORED;
        assert_eq!(
            summary(&barriers(from, to, FAMILY)?),
            [(
                vk::ImageLayout::UNDEFINED,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                ignored,
                ignored
            )]
        );
        Ok(())
    }

    #[test]
    fn a_release_changes_the_layout_first_and_hands_over_in_the_new_layout() -> Result<(), Error> {
        let from = state(vk::ImageLayout::TRANSFER_SRC_OPTIMAL, QueueFamily::Device);
        let to = state(vk::ImageLayout::GENERAL, QueueFamily::External);
        let ignored = vk::QUEUE_FAMILY_IGNORED;
        assert_eq!(
            summary(&barriers(from, to, FAMILY)?),
            [
                (
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    vk::ImageLayout::GENERAL,
                    ignored,
                    ignored
                ),
                (
                    vk::ImageLayout::GENERAL,
                    vk::ImageLayout::GENERAL,
                    FAMILY,
                    vk::QUEUE_FAMILY_EXTERNAL
                ),
            ]
        );
        Ok(())
    }

    #[test]
    fn an_acquire_takes_over_in_the_source_layout_then_changes_the_layout() -> Result<(), Error> {
        let from = state(vk::ImageLayout::GENERAL, QueueFamily::External);
        let to = state(vk::ImageLayout::TRANSFER_SRC_OPTIMAL, QueueFamily::Device);
        let ignored = vk::QUEUE_FAMILY_IGNORED;
        assert_eq!(
            summary(&barriers(from, to, FAMILY)?),
            [
                (
                    vk::ImageLayout::GENERAL,
                    vk::ImageLayout::GENERAL,
                    vk::QUEUE_FAMILY_EXTERNAL,
                    FAMILY
                ),
                (
                    vk::ImageLayout::GENERAL,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    ignored,
                    ignored
                ),
            ]
        );
        Ok(())
    }

    #[test]
    fn an_image_outside_the_queue_cannot_change_its_layout() {
        let from = state(vk::ImageLayout::GENERAL, QueueFamily::External);
        let to = state(vk::ImageLayout::TRANSFER_SRC_OPTIMAL, QueueFamily::External);
        assert!(matches!(
            barriers(from, to, FAMILY),
            Err(Error::ExternalTransition)
        ));
    }

    #[test]
    fn no_image_moves_into_an_undefined_layout() {
        let from = state(vk::ImageLayout::GENERAL, QueueFamily::Device);
        let to = state(vk::ImageLayout::UNDEFINED, QueueFamily::Device);
        assert!(matches!(
            barriers(from, to, FAMILY),
            Err(Error::TargetLayout(vk::ImageLayout::UNDEFINED))
        ));
    }
}
