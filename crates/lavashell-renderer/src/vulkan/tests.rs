// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::test_error::TestError;
use ash::vk::Handle;
use lavashell_core::scene::tree::SceneTree;
use lavashell_effect_quality::{DrawId, DrawTimeKey, EffectId, VariantId};
use lavashell_vulkan::{Commands, Image, ImageDescription, Recording};
use skia_safe::gpu::vk as skvk;
use std::sync::Arc;
use std::time::Duration;

const SIDE: u32 = 8;

const DRAW: DrawTimeKey = DrawTimeKey {
    draw: DrawId {
        surface: SurfaceId(0),
        leaf: SceneTree::LEAF,
        ordinal: 0,
        effect: EffectId(42),
    },
    variant: VariantId(1),
};

const USAGE: vk::ImageUsageFlags = vk::ImageUsageFlags::from_raw(
    vk::ImageUsageFlags::COLOR_ATTACHMENT.as_raw()
        | vk::ImageUsageFlags::SAMPLED.as_raw()
        | vk::ImageUsageFlags::TRANSFER_SRC.as_raw()
        | vk::ImageUsageFlags::TRANSFER_DST.as_raw(),
);

pub(super) fn description(format: vk::Format) -> ImageDescription {
    ImageDescription {
        format,
        extent: vk::Extent2D {
            width: SIDE,
            height: SIDE,
        },
        usage: USAGE,
    }
}

/// A recording that clears `image` red and leaves it in `TRANSFER_DST_OPTIMAL`.
pub(super) fn cleared_red(image: &Arc<Image>) -> Result<Recording, TestError> {
    let mut recording = Commands::new(image.device())?.record()?;
    recording.discard(image, vk::ImageLayout::TRANSFER_DST_OPTIMAL)?;
    let red = vk::ClearColorValue {
        float32: [1.0, 0.0, 0.0, 1.0],
    };
    recording.clear(image, red)?;
    Ok(recording)
}

const RED: [u8; 4] = [255, 0, 0, 255];

const GREEN: [u8; 4] = [0, 255, 0, 255];

/// Uploads `color`, in unpremultiplied sRGB RGBA, into every pixel of `canvas`.
fn fill(canvas: &Canvas, color: [u8; 4]) -> Result<(), TestError> {
    let size = canvas.base_layer_size();
    let info = skia_safe::ImageInfo::new(
        size,
        ColorType::RGBA8888,
        skia_safe::AlphaType::Unpremul,
        skia_safe::ColorSpace::new_srgb(),
    );
    let pixels = color.repeat(usize::try_from(size.width * size.height)?);
    assert!(canvas.write_pixels(&info, &pixels, info.min_row_bytes(), (0, 0)));
    Ok(())
}

fn pixels(png: &[u8]) -> Result<Vec<[u8; 4]>, TestError> {
    let image =
        skia_safe::Image::from_encoded(skia_safe::Data::new_copy(png)).ok_or(Error::DecodeImage)?;
    let info = skia_safe::ImageInfo::new(
        image.dimensions(),
        ColorType::RGBA8888,
        skia_safe::AlphaType::Unpremul,
        skia_safe::ColorSpace::new_srgb(),
    );
    let mut pixels = vec![0; info.compute_min_byte_size()];
    assert!(image.read_pixels(
        &info,
        &mut pixels,
        info.min_row_bytes(),
        (0, 0),
        skia_safe::image::CachingHint::Disallow
    ));
    Ok(pixels.as_chunks::<4>().0.to_vec())
}

pub(super) fn assert_red(png: &[u8]) -> Result<(), TestError> {
    let pixels = pixels(png)?;
    assert_eq!(pixels.len(), (SIDE * SIDE) as usize);
    let stray = pixels.iter().position(|pixel| *pixel != RED);
    assert_eq!(stray, None, "{:?}", stray.map(|index| pixels[index]));
    Ok(())
}

#[test]
fn timestamps_poll_unfinished_tokens_and_reclaim_only_completed_slots() -> Result<(), TestError> {
    let mut gpu = VulkanRenderer::headless()?;
    let profiler = gpu.create_profiler()?;
    let token = profiler.borrow_mut().begin(SampleTag {
        kind: SampleKind::Effect(DRAW),
        ..SampleTag::frame(SurfaceId(0), 1)
    })?;
    let pending = profiler.borrow_mut().collect();
    assert!(pending.samples.is_empty() && pending.failures.is_empty());
    let mut surface = gpu.offscreen((96, 48))?;
    fill(surface.canvas(), GREEN)?;
    gpu.context
        .flush_and_submit_surface(&mut surface, Some(SyncCpu::No));
    profiler.borrow_mut().end(token)?;
    gpu.device.wait_idle()?;
    let Collected { samples, failures } = profiler.borrow_mut().collect();
    assert!(failures.is_empty());
    assert_eq!(samples.len(), 1);
    assert_eq!(samples[0].tag.kind, SampleKind::Effect(DRAW));
    assert!(samples[0].elapsed > Duration::ZERO);
    assert!(profiler.borrow_mut().collect().samples.is_empty());
    Ok(())
}

#[test]
fn shutdown_retires_three_unpolled_scene_submissions_inside_context_scope() -> Result<(), TestError>
{
    let mut gpu = VulkanRenderer::headless()?;
    let mut mailbox = SceneMailbox::new(3);
    for generation in 1..=3 {
        let mut surface = gpu.offscreen((96, 48))?;
        fill(surface.canvas(), GREEN)?;
        let pending = gpu.submit_scene(&mut surface, generation, Duration::ZERO)?;
        assert!(mailbox.publish(pending.scene, pending.completion).is_ok());
    }
    assert_eq!(mailbox.pending(), 3);
    assert!(mailbox.latest().is_none());
    drop(mailbox);
    let mut surface = gpu.offscreen((SIDE, SIDE))?;
    fill(surface.canvas(), RED)?;
    assert_red(&gpu.png(&mut surface)?)
}

#[test]
fn an_offscreen_surface_outlives_its_renderer() -> Result<(), TestError> {
    let mut gpu = VulkanRenderer::headless()?;
    let mut surface = gpu.offscreen((SIDE, SIDE))?;
    fill(surface.canvas(), RED)?;
    drop(gpu);
    drop(surface);
    Ok(())
}

#[test]
fn an_embedded_renderer_paints_into_an_image_of_the_host() -> Result<(), TestError> {
    let mut host = VulkanRenderer::headless()?;
    let target = Image::new(&host.device, &description(vk::Format::R8G8B8A8_UNORM))?;
    // SAFETY: this thread is the only one that submits to the queue of the host renderer.
    let mut embedded = unsafe { EmbeddedVulkanRenderer::new(host.device.clone()) }?;
    let image = BorrowedImage {
        image: target.handle(),
        size: (SIDE, SIDE),
        format: vk::Format::R8G8B8A8_UNORM,
        usage: USAGE,
        layout: vk::ImageLayout::UNDEFINED,
        return_layout: vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        ready: ReadySignal::Unsignaled,
    };
    image.check()?;
    let mut filled = Ok(());
    let paint = |canvas: &Canvas| filled = fill(canvas, RED);
    // SAFETY: `target`, created on the host device, outlives the embedded renderer. Dropping the embedded renderer
    // waits until the draw is complete.
    unsafe { embedded.draw(image, paint) }?;
    filled?;
    drop(embedded);
    let painted = skia_handoff::SkiaImage {
        image: target.handle(),
        layout: skvk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        format: skvk::Format::R8G8B8A8_UNORM,
        usage: USAGE,
        queue_family: vk::QUEUE_FAMILY_IGNORED,
        conversion: None,
    };
    let side = i32::try_from(SIDE)?;
    // SAFETY: `target` outlives the encoding, which reads the texture back before it returns.
    let texture =
        unsafe { skia_handoff::backend_texture(&painted, (side, side), "embedded target") }?;
    let painted = gpu::images::borrow_texture_from(
        &mut host.context,
        &texture,
        SurfaceOrigin::TopLeft,
        ColorType::RGBA8888,
        skia_safe::AlphaType::Premul,
        None,
    )
    .ok_or(TestError::Missing("borrowed texture"))?;
    let png = painted
        .encode(&mut *host.context, skia_safe::EncodedImageFormat::PNG, None)
        .ok_or(Error::EncodePng)?;
    assert_red(png.as_bytes())
}

#[test]
fn a_borrowed_image_is_checked_for_what_skia_needs_to_draw_into_it() {
    let drawable = BorrowedImage {
        image: vk::Image::from_raw(1),
        size: (SIDE, SIDE),
        format: vk::Format::R8G8B8A8_UNORM,
        usage: USAGE,
        layout: vk::ImageLayout::UNDEFINED,
        return_layout: vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        ready: ReadySignal::Unsignaled,
    };
    assert!(drawable.check().is_ok());
    let null = BorrowedImage {
        image: vk::Image::null(),
        ..drawable
    };
    assert!(matches!(null.check(), Err(Error::EmbeddedImageHandle)));
    let sampled = BorrowedImage {
        usage: vk::ImageUsageFlags::SAMPLED,
        ..drawable
    };
    assert!(matches!(sampled.check(), Err(Error::EmbeddedImageUsage(_))));
    let depth = BorrowedImage {
        format: vk::Format::D32_SFLOAT,
        ..drawable
    };
    assert!(matches!(depth.check(), Err(Error::EmbeddedFormat(_))));
    for size in [(0, SIDE), (SIDE, 0), (u32::MAX, SIDE)] {
        let image = BorrowedImage { size, ..drawable };
        assert!(matches!(image.check(), Err(Error::EmbeddedImageSize(_))));
    }
}

#[test]
fn an_image_exported_by_another_device_is_drawn_without_a_copy() -> Result<(), TestError> {
    let producer = VulkanRenderer::headless()?;
    let mut consumer = VulkanRenderer::headless()?;
    let source = Image::exportable(&producer.device, &description(vk::Format::R8G8B8A8_SRGB))?;
    let external = ExternalImage::release(cleared_red(source.image())?, &source, Arc::new(()))?;
    let pending = consumer.import_scene(external, 1, Duration::ZERO)?;
    assert_eq!(pending.completion.status()?, Status::Complete);
    let mut surface = consumer.offscreen((SIDE, SIDE))?;
    pending.scene.content.layer_of(surface.canvas())?.draw();
    let png = consumer.png(&mut surface)?;
    assert_red(&png)?;
    drop(pending);
    consumer.poll_scene_imports()?;
    drop(consumer);
    Ok(())
}
