// SPDX-License-Identifier: GPL-3.0-only
use super::super::tests::{cleared_red, description};
use super::*;
use crate::test_error::TestError;
use ash::vk;
use lavashell_vulkan::Image;

fn imported(gpu: &VulkanRenderer) -> usize {
    gpu.imported.borrow().allocations.len()
}

/// Hands back the imported images that no scene reads and waits until the GPU is idle. The second poll then gives
/// the handed-back images to their producers and frees the evicted images.
fn settle(gpu: &mut VulkanRenderer) -> Result<(), TestError> {
    gpu.poll_scene_imports()?;
    gpu.device.wait_idle()?;
    gpu.poll_scene_imports()?;
    Ok(())
}

#[test]
fn an_image_is_imported_once_while_its_producer_keeps_it() -> Result<(), TestError> {
    let producer = VulkanRenderer::headless()?;
    let mut consumer = VulkanRenderer::headless()?;
    let format = vk::Format::R8G8B8A8_SRGB;
    let source = Image::exportable(&producer.device, &description(format))?;
    for generation in 1..=2 {
        let lease = Arc::new(());
        let external =
            ExternalImage::release(cleared_red(source.image())?, &source, lease.clone())?;
        let pending = consumer.import_scene(external, generation, Duration::ZERO)?;
        drop(pending);
        settle(&mut consumer)?;
        assert_eq!(Arc::strong_count(&lease), 1);
        assert_eq!(imported(&consumer), 1);
    }
    Ok(())
}

#[test]
fn an_image_is_dropped_once_its_producer_dropped_it_and_no_scene_reads_it() -> Result<(), TestError>
{
    let producer = VulkanRenderer::headless()?;
    let mut consumer = VulkanRenderer::headless()?;
    let format = vk::Format::R8G8B8A8_SRGB;
    let source = Image::exportable(&producer.device, &description(format))?;
    let external = ExternalImage::release(cleared_red(source.image())?, &source, Arc::new(()))?;
    let pending = consumer.import_scene(external, 1, Duration::ZERO)?;
    drop(source);
    settle(&mut consumer)?;
    assert_eq!(imported(&consumer), 1);
    drop(pending);
    settle(&mut consumer)?;
    assert_eq!(imported(&consumer), 0);
    Ok(())
}

#[test]
fn a_polled_release_is_complete_once_its_queue_is_idle_and_imports() -> Result<(), TestError> {
    let producer = VulkanRenderer::headless()?;
    let mut consumer = VulkanRenderer::headless()?;
    let format = vk::Format::R8G8B8A8_SRGB;
    let source = Image::exportable(&producer.device, &description(format))?;
    let pending = PendingRelease::submit(cleared_red(source.image())?, &source, Arc::new(()))?;
    producer.device.wait_idle()?;
    let ReleaseProgress::Complete(completed) = pending.poll()? else {
        return Err(TestError::Missing("completed release"));
    };
    let _imported_scene = consumer.import_scene(completed.image, 1, Duration::ZERO)?;
    assert_eq!(imported(&consumer), 1);
    Ok(())
}

#[test]
fn an_image_is_not_taken_over_again_while_a_scene_reads_it() -> Result<(), TestError> {
    let producer = VulkanRenderer::headless()?;
    let mut consumer = VulkanRenderer::headless()?;
    let format = vk::Format::R8G8B8A8_SRGB;
    let source = Image::exportable(&producer.device, &description(format))?;
    let release = || -> Result<ExternalImage, TestError> {
        let recording = cleared_red(source.image())?;
        Ok(ExternalImage::release(recording, &source, Arc::new(()))?)
    };
    let pending = consumer.import_scene(release()?, 1, Duration::ZERO)?;
    let again = consumer.import_scene(release()?, 2, Duration::ZERO);
    assert!(matches!(again, Err(Error::SceneInUse)));
    drop(pending);
    settle(&mut consumer)?;
    assert!(consumer.import_scene(release()?, 3, Duration::ZERO).is_ok());
    Ok(())
}
