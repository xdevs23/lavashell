// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::effect_registry::{EffectInput, ShapePainter, SurfaceAccess};
use crate::failure::Failures;
use crate::resources::PaintResources;
use crate::test_error::TestError;
use crate::{FieldRoom, UniformRange};
use skia_safe::{AlphaType, ColorType, ImageInfo, Surface};

pub(super) fn draw_on(surface: SurfaceId) -> DrawId {
    DrawId {
        surface,
        leaf: SceneTree::LEAF,
        ordinal: 0,
        effect: ID,
    }
}

pub(super) fn pixels(surface: &mut Surface) -> Vec<u8> {
    let info = ImageInfo::new((96, 48), ColorType::RGBA8888, AlphaType::Premul, None);
    let mut bytes = vec![0; 96 * 48 * 4];
    assert!(surface.read_pixels(&info, &mut bytes, 96 * 4, (0, 0)));
    bytes
}

pub(super) fn background(canvas: &Canvas) {
    let _chained_canvas = canvas.clear(skia_safe::Color::BLACK);
    let mut paint = Paint::default();
    for x in 0..24_u16 {
        let _chained_paint = paint.set_color(if x % 2 == 0 {
            skia_safe::Color::WHITE
        } else {
            skia_safe::Color::BLUE
        });
        let _chained_canvas = canvas.draw_rect(
            skia_safe::Rect::from_xywh(f32::from(x) * 4.0, 0.0, 4.0, 48.0),
            &paint,
        );
    }
}

fn setup() -> Result<(Backend, DrawCommand, EffectInput), TestError> {
    let glass = GlassPrograms::new(&FieldRoom::new(UniformRange::VULKAN_REQUIRED)?)?;
    let shared = Rc::new(RefCell::new(Shared::new(glass)));
    Ok((
        Backend { shared },
        DrawCommand::Backdrop {
            shape: Shape::rounded(
                Rect::new(-3.25.dp(), 4.125.dp(), 63.5.dp(), 30.25.dp()),
                5.dp(),
            ),
            effect: Backdrop::glass(
                3.dp(),
                lavashell_core::effects::Glass {
                    refraction: constant!(NonNegativeDp::try_new(Dp::new(2.0))),
                    dispersion: constant!(NonNegativeDp::try_new(Dp::new(0.5))),
                    rim: constant!(NonNegative::try_new(0.2)),
                },
            )?,
        },
        EffectInput {
            scene_generation: 1,
            cache_allowed: true,
        },
    ))
}

#[test]
fn the_blur_halves_per_level_until_it_spans_less_than_a_device_pixel() -> Result<(), TestError> {
    let (backend, command, _input_of_a_drawn_frame) = setup()?;
    let target = |pixels_per_dp| DrawTarget { pixels_per_dp };
    let offered = |pixels_per_dp| backend.variants(&command, target(pixels_per_dp));
    let ladder = |pixels_per_dp| {
        let variants = offered(pixels_per_dp).into_iter();
        variants.map(|offered| offered.variant).collect::<Vec<_>>()
    };
    let blur = |level| Mode::Blur(level).variant();
    assert_eq!(ladder(1.0), [blur(0), blur(1), CACHED, TINT]);
    assert_eq!(ladder(2.0), [blur(0), blur(1), blur(2), CACHED, TINT]);
    assert_eq!(ladder(0.25), [blur(0), CACHED, TINT]);
    for variant in ladder(2.0) {
        assert_eq!(Mode::of(variant).map(Mode::variant), Some(variant));
    }
    let qualities = offered(2.0).into_iter().map(|offered| offered.quality);
    let qualities = qualities.collect::<Vec<_>>();
    assert!(qualities.is_sorted_by(|finer, coarser| finer > coarser));
    Ok(())
}

#[test]
fn a_stored_image_evicts_the_least_recently_used_images_until_it_fits() -> Result<(), TestError> {
    let glass = GlassPrograms::new(&FieldRoom::new(UniformRange::VULKAN_REQUIRED)?)?;
    let mut shared = Shared::new(glass);
    let image = skia_safe::surfaces::raster_n32_premul((1, 1))
        .ok_or(TestError::RasterSurface)?
        .image_snapshot();
    let cached = |bytes| Cached {
        surface: SurfaceId::numbered(1),
        image: image.clone(),
        optics: Shape::rounded(Rect::new(0.dp(), 0.dp(), 1.dp(), 1.dp()), 0.dp()),
        effect: constant!(Backdrop::blur(Dp::new(1.0))),
        matrix: [0; 9],
        bytes,
        device_bounds: skia_safe::IRect::new_empty(),
        scene_generation: 1,
    };
    let [first, second, third] = [1, 2, 3].map(|surface| draw_on(SurfaceId::numbered(surface)));
    for id in [first, second] {
        assert_eq!(shared.store(id, cached(2), 4).bytes, 2);
    }
    assert!(shared.entries.get(&first).is_some());
    assert_eq!(shared.store(third, cached(2), 4).bytes, 2);
    assert!(shared.entries.contains(&first) && shared.entries.contains(&third));
    assert!(!shared.entries.contains(&second));
    assert_eq!(shared.bytes, 4);
    assert_eq!(shared.store(first, cached(3), 4).bytes, 3);
    assert_eq!((shared.entries.len(), shared.bytes), (1, 3));
    Ok(())
}

#[test]
fn fractional_offscreen_snapshot_replays_without_stretching() -> Result<(), TestError> {
    compare(
        &mut skia_safe::surfaces::raster_n32_premul((96, 48)).ok_or(TestError::RasterSurface)?,
        &mut skia_safe::surfaces::raster_n32_premul((96, 48)).ok_or(TestError::RasterSurface)?,
        true,
    )
}

#[test]
fn vulkan_cached_glass_preserves_optics_and_fractional_geometry() -> Result<(), TestError> {
    let mut gpu = crate::VulkanRenderer::headless()?;
    for clip in [false, true] {
        compare(
            &mut *gpu.offscreen(PixelSize::new(96, 48))?,
            &mut *gpu.offscreen(PixelSize::new(96, 48))?,
            clip,
        )?;
    }
    Ok(())
}

fn prepare(canvas: &Canvas, clip: bool) {
    background(canvas);
    let _chained_canvas = canvas.scale((1.25, 1.25));
    if clip {
        let _chained_canvas = canvas.clip_rect(
            skia_safe::Rect::from_xywh(10.0, 10.0, 45.0, 20.0),
            None,
            false,
        );
    }
}

struct ChannelMismatch {
    x: usize,
    y: usize,
    channel: usize,
    actual: u8,
    expected: u8,
}

struct PixelDifference {
    worst: u8,
    first_mismatches: Vec<ChannelMismatch>,
}

impl std::fmt::Display for ChannelMismatch {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "({}, {}) channel {}: {} against {}",
            self.x, self.y, self.channel, self.actual, self.expected
        )
    }
}

impl std::fmt::Display for PixelDifference {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "largest channel difference {}", self.worst)?;
        self.first_mismatches
            .iter()
            .try_for_each(|mismatch| write!(formatter, "; {mismatch}"))
    }
}

impl PixelDifference {
    fn between(actual: &[u8], expected: &[u8]) -> Result<Self, TestError> {
        let channels = || actual.iter().zip(expected);
        let worst = channels()
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .ok_or(TestError::Missing("pixels"))?;
        let first_mismatches = channels()
            .enumerate()
            .filter(|(_, (a, b))| a.abs_diff(**b) > 2)
            .map(|(index, (&actual, &expected))| ChannelMismatch {
                x: index / 4 % 96,
                y: index / 4 / 96,
                channel: index % 4,
                actual,
                expected,
            })
            .take(12)
            .collect();
        Ok(Self {
            worst,
            first_mismatches,
        })
    }
}

fn compare(surface: &mut Surface, reference: &mut Surface, clip: bool) -> Result<(), TestError> {
    let (resources, failures) = (
        PaintResources::new(
            &FieldRoom::new(UniformRange::VULKAN_REQUIRED)?,
            Failures::default(),
        )?,
        Failures::default(),
    );
    let (mut backend, command, input) = setup()?;
    let _save_count = surface.canvas().save();
    prepare(surface.canvas(), clip);
    let request = |variant| EffectRequest {
        surface: SurfaceId::numbered(0),
        id: draw_on(SurfaceId::numbered(0)),
        variant,
        command: &command,
        input: Some(&input),
        access: SurfaceAccess::Cacheable,
        painter: ShapePainter::new(&resources, &failures),
    };
    backend.render(surface.canvas(), &request(FULL_BLUR));
    assert_eq!(backend.shared.borrow().entries.len(), 1);
    let captured = pixels(surface);
    prepare(reference.canvas(), clip);
    let DrawCommand::Backdrop { shape, effect } = &command else {
        return Err(TestError::Missing("backdrop command"));
    };
    GlassPrograms::new(&FieldRoom::new(UniformRange::VULKAN_REQUIRED)?)?.backdrop(
        reference.canvas(),
        &PaintResources::new(
            &FieldRoom::new(UniformRange::VULKAN_REQUIRED)?,
            Failures::default(),
        )?,
        shape,
        BlurLevel::new(effect, 0),
        SurfaceAccess::Current,
    )?;
    let expected = pixels(reference);
    let filled = PixelDifference::between(&captured, &expected)?;
    assert!(
        filled.worst <= 2,
        "cache fill differs from ordinary glass: {filled}"
    );
    let _chained_canvas = surface.canvas().restore_to_count(1);
    let _chained_canvas = surface.canvas().reset_matrix();
    prepare(surface.canvas(), clip);
    backend.render(surface.canvas(), &request(CACHED));
    let replayed = pixels(surface);
    let replay = PixelDifference::between(&captured, &replayed)?;
    assert!(replay.worst <= 2, "clip {clip}, replay differs: {replay}");
    Ok(())
}

#[test]
fn cached_output_rejects_new_opacity_destination_and_a_new_scene() -> Result<(), TestError> {
    let (resources, failures) = (
        PaintResources::new(
            &FieldRoom::new(UniformRange::VULKAN_REQUIRED)?,
            Failures::default(),
        )?,
        Failures::default(),
    );
    let (mut backend, command, mut input) = setup()?;
    let mut surface =
        skia_safe::surfaces::raster_n32_premul((96, 48)).ok_or(TestError::RasterSurface)?;
    background(surface.canvas());
    backend.render(
        surface.canvas(),
        &EffectRequest {
            surface: SurfaceId::numbered(0),
            id: draw_on(SurfaceId::numbered(0)),
            variant: FULL_BLUR,
            command: &command,
            input: Some(&input),
            access: SurfaceAccess::Cacheable,
            painter: ShapePainter::new(&resources, &failures),
        },
    );
    background(surface.canvas());
    let _save_count = surface.canvas().save_layer_alpha_f(None, 0.5);
    let before = pixels(&mut surface);
    backend.render(
        surface.canvas(),
        &EffectRequest {
            surface: SurfaceId::numbered(0),
            id: draw_on(SurfaceId::numbered(0)),
            variant: CACHED,
            command: &command,
            input: Some(&input),
            access: SurfaceAccess::Unreadable,
            painter: ShapePainter::new(&resources, &failures),
        },
    );
    let _chained_canvas = surface.canvas().restore();
    assert_eq!(pixels(&mut surface), before);
    input.scene_generation += 1;
    let before = pixels(&mut surface);
    backend.render(
        surface.canvas(),
        &EffectRequest {
            surface: SurfaceId::numbered(0),
            id: draw_on(SurfaceId::numbered(0)),
            variant: CACHED,
            command: &command,
            input: Some(&input),
            access: SurfaceAccess::Cacheable,
            painter: ShapePainter::new(&resources, &failures),
        },
    );
    assert_eq!(pixels(&mut surface), before);
    Ok(())
}

#[test]
fn cached_pixels_and_retirement_respect_the_window_surface() -> Result<(), TestError> {
    let (resources, failures) = (
        PaintResources::new(
            &FieldRoom::new(UniformRange::VULKAN_REQUIRED)?,
            Failures::default(),
        )?,
        Failures::default(),
    );
    let (mut backend, command, input) = setup()?;
    let mut first =
        skia_safe::surfaces::raster_n32_premul((96, 48)).ok_or(TestError::RasterSurface)?;
    background(first.canvas());
    backend.render(
        first.canvas(),
        &EffectRequest {
            surface: SurfaceId::numbered(11),
            id: draw_on(SurfaceId::numbered(11)),
            variant: FULL_BLUR,
            command: &command,
            input: Some(&input),
            access: SurfaceAccess::Cacheable,
            painter: ShapePainter::new(&resources, &failures),
        },
    );
    let mut second =
        skia_safe::surfaces::raster_n32_premul((96, 48)).ok_or(TestError::RasterSurface)?;
    let _chained_canvas = second.canvas().clear(skia_safe::Color::RED);
    let before = pixels(&mut second);
    backend.render(
        second.canvas(),
        &EffectRequest {
            surface: SurfaceId::numbered(21),
            id: draw_on(SurfaceId::numbered(21)),
            variant: CACHED,
            command: &command,
            input: Some(&input),
            access: SurfaceAccess::Cacheable,
            painter: ShapePainter::new(&resources, &failures),
        },
    );
    assert_eq!(pixels(&mut second), before);
    backend.forget_surface(SurfaceId::numbered(21));
    assert_eq!(backend.shared.borrow().entries.len(), 1);
    backend.forget_surface(SurfaceId::numbered(11));
    assert!(backend.shared.borrow().entries.is_empty());
    Ok(())
}
