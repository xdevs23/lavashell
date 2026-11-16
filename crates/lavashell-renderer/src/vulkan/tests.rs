// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::test_error::TestError;
use crate::test_surfaces::{EFFECT_SURFACE, TestSurface, encoded_pixels};
use ash::vk::Handle;
use lavashell_core::scene::tree::SceneTree;
use lavashell_core::surface::SurfaceId;
use lavashell_effect_quality::{DrawId, DrawTimeKey, EffectId, VariantId};
use lavashell_vulkan::{Commands, Image, ImageDescription, Recording};
use skia_safe::ColorType;
use skia_safe::gpu::{SurfaceOrigin, vk as skvk};
use std::sync::Arc;
use std::time::Duration;

const SURFACE: TestSurface = TestSurface::new(8, 8);

const DRAW: DrawTimeKey = DrawTimeKey {
    draw: DrawId {
        surface: SurfaceId::numbered(0),
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
    let PixelSize { width, height } = SURFACE.pixel_size();
    ImageDescription {
        format,
        extent: vk::Extent2D { width, height },
        usage: USAGE,
    }
}

pub(super) struct TestLease;

impl SceneLease for TestLease {}

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

pub(super) fn assert_red(png: &[u8]) -> Result<(), TestError> {
    let pixels = encoded_pixels(png, skia_safe::AlphaType::Unpremul)?;
    assert_eq!(u64::try_from(pixels.iter().len())?, SURFACE.pixel_count());
    assert_eq!(pixels.iter().find(|pixel| **pixel != RED), None);
    Ok(())
}

#[test]
fn timestamps_poll_unfinished_tokens_and_reclaim_only_completed_slots() -> Result<(), TestError> {
    let mut gpu = VulkanRenderer::headless()?;
    let mut profiler = gpu.create_profiler()?;
    let token = profiler.begin(SampleTag {
        kind: SampleKind::Effect(DRAW),
        ..SampleTag::frame(SurfaceId::numbered(0), 1)
    })?;
    let pending = profiler.collect();
    assert!(pending.samples.is_empty() && pending.failures.is_empty());
    let mut surface = gpu.offscreen(EFFECT_SURFACE.pixel_size())?;
    fill(surface.canvas(), GREEN)?;
    gpu.context
        .flush_and_submit_surface(&mut surface, Some(SyncCpu::No));
    profiler.end(token)?;
    gpu.context.device().wait_idle()?;
    let Collected { samples, failures } = profiler.collect();
    assert!(failures.is_empty());
    assert_eq!(samples.len(), 1);
    assert_eq!(samples[0].tag.kind, SampleKind::Effect(DRAW));
    assert!(samples[0].elapsed > Duration::ZERO);
    assert!(profiler.collect().samples.is_empty());
    Ok(())
}

#[test]
fn shutdown_retires_three_unpolled_scene_submissions_inside_context_scope() -> Result<(), TestError>
{
    let mut gpu = VulkanRenderer::headless()?;
    let mut mailbox = SceneMailbox::new(3);
    for generation in 1..=3 {
        let mut surface = gpu.offscreen(EFFECT_SURFACE.pixel_size())?;
        fill(surface.canvas(), GREEN)?;
        let pending = gpu.submit_scene(&mut surface, generation, Duration::ZERO)?;
        assert!(mailbox.publish(pending.scene, pending.completion).is_ok());
    }
    assert_eq!(mailbox.pending(), 3);
    assert!(mailbox.latest().is_none());
    drop(mailbox);
    let mut surface = gpu.offscreen(SURFACE.pixel_size())?;
    fill(surface.canvas(), RED)?;
    assert_red(&gpu.png(&mut surface)?)
}

#[test]
fn an_offscreen_surface_outlives_its_renderer() -> Result<(), TestError> {
    let mut gpu = VulkanRenderer::headless()?;
    let mut surface = gpu.offscreen(SURFACE.pixel_size())?;
    fill(surface.canvas(), RED)?;
    drop(gpu);
    drop(surface);
    Ok(())
}

#[test]
fn an_embedded_renderer_paints_into_an_image_of_the_host() -> Result<(), TestError> {
    let mut host = VulkanRenderer::headless()?;
    let target = Image::new(
        host.context.device(),
        &description(vk::Format::R8G8B8A8_UNORM),
    )?;
    // SAFETY: the embedded renderer exists to draw into images of a host device, and only the host can promise that
    // nothing else submits to its queue. This thread is the only one that submits to the queue of the host renderer.
    let mut embedded = unsafe { EmbeddedVulkanRenderer::new(host.context.device()) }?;
    let image = BorrowedImage {
        image: target.handle(),
        size: SURFACE.pixel_size(),
        format: vk::Format::R8G8B8A8_UNORM,
        usage: USAGE,
        layout: vk::ImageLayout::UNDEFINED,
        return_layout: vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        ready: ReadySignal::Unsignaled,
    };
    image.check()?;
    let mut filled = Ok(());
    let paint = |canvas: &Canvas| filled = fill(canvas, RED);
    // SAFETY: drawing into a host image cannot be checked by the embedded renderer, because only the host knows the
    // image and its layout. `target`, created on the host device in `UNDEFINED` layout, outlives the embedded
    // renderer. Dropping the embedded renderer waits until the draw is complete.
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
    let skia_safe::ISize { width, height } = SURFACE.dimensions();
    // SAFETY: skia-safe has no safe way to wrap a Vulkan image, because Skia keeps the image without a lifetime.
    // `target` outlives the encoding, which reads the texture back before it returns.
    let texture =
        unsafe { skia_handoff::backend_texture(&painted, (width, height), "embedded target") }?;
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
        size: SURFACE.pixel_size(),
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
    let valid = SURFACE.pixel_size();
    for size in [
        PixelSize { width: 0, ..valid },
        PixelSize { height: 0, ..valid },
        PixelSize {
            width: u32::MAX,
            ..valid
        },
    ] {
        let image = BorrowedImage { size, ..drawable };
        assert!(matches!(image.check(), Err(Error::EmbeddedImageSize(_))));
    }
}

#[test]
fn an_image_exported_by_another_device_is_drawn_without_a_copy() -> Result<(), TestError> {
    let producer = VulkanRenderer::headless()?;
    let mut consumer = VulkanRenderer::headless()?;
    let source = Image::exportable(
        producer.context.device(),
        &description(vk::Format::R8G8B8A8_SRGB),
    )?;
    let external =
        ExternalImage::release(cleared_red(source.image())?, &source, Arc::new(TestLease))?;
    let pending = consumer.import_scene(external, 1, Duration::ZERO)?;
    assert_eq!(pending.completion.status()?, Status::Complete);
    let mut surface = consumer.offscreen(SURFACE.pixel_size())?;
    pending.scene.content.layer_of(surface.canvas())?.draw();
    let png = consumer.png(&mut surface)?;
    assert_red(&png)?;
    drop(pending);
    consumer.poll_scene_imports()?;
    drop(consumer);
    Ok(())
}

#[test]
fn explicit_times_need_a_device_with_display_timing() -> Result<(), TestError> {
    let gpu = VulkanRenderer::headless()?;
    assert!(matches!(
        gpu.presentation_time(Duration::from_nanos(1)),
        Err(Error::NoDisplayTiming)
    ));
    Ok(())
}
