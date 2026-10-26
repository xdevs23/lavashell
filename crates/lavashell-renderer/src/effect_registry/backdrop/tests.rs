// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::destination::Destination;
use crate::effect_registry::{EffectInput, ShapePainter, SurfaceAccess};
use crate::effects_tests::stripes;
use crate::failure::{DrawFailure, Failures};
use crate::field::test_shapes::{Distance, ellipse, union};
use crate::resources::PaintResources;
use crate::test_error::TestError;
use crate::{FieldRoom, UniformRange};
use skia_safe::{AlphaType, ColorType, ImageInfo, Surface};

/// The identifier of the first glass draw in the only leaf of a scene tree on `surface`.
pub(super) fn draw_on(surface: SurfaceId) -> DrawId {
    DrawId {
        surface,
        leaf: SceneTree::LEAF,
        ordinal: 0,
        effect: ID,
    }
}

fn pixels(surface: &mut Surface) -> Vec<u8> {
    let info = ImageInfo::new((96, 48), ColorType::RGBA8888, AlphaType::Premul, None);
    let mut bytes = vec![0; 96 * 48 * 4];
    assert!(surface.read_pixels(&info, &mut bytes, 96 * 4, (0, 0)));
    bytes
}

fn background(canvas: &Canvas) {
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
    let glass = GlassPrograms::new(&FieldRoom::new(UniformRange::VULKAN_REQUIRED))?;
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
    let (backend, command, _) = setup()?;
    let target = |pixels_per_dp| DrawTarget { pixels_per_dp };
    let ladder = |pixels_per_dp| backend.variants(&command, target(pixels_per_dp));
    let blur = |level| Mode::Blur(level).variant();
    assert_eq!(ladder(1.0), [blur(0), blur(1), CACHED, TINT]);
    assert_eq!(ladder(2.0), [blur(0), blur(1), blur(2), CACHED, TINT]);
    assert_eq!(ladder(0.25), [blur(0), CACHED, TINT]);
    for variant in ladder(2.0) {
        assert_eq!(Mode::of(variant).map(Mode::variant), Some(variant));
    }
    Ok(())
}

#[test]
fn a_stored_image_evicts_the_least_recently_used_images_until_it_fits() -> Result<(), TestError> {
    let glass = GlassPrograms::new(&FieldRoom::new(UniformRange::VULKAN_REQUIRED))?;
    let mut shared = Shared::new(glass);
    let image = skia_safe::surfaces::raster_n32_premul((1, 1))
        .ok_or(TestError::RasterSurface)?
        .image_snapshot();
    let cached = |bytes| Cached {
        surface: SurfaceId(1),
        image: image.clone(),
        optics: Shape::rounded(Rect::new(0.dp(), 0.dp(), 1.dp(), 1.dp()), 0.dp()),
        effect: constant!(Backdrop::blur(Dp::new(1.0))),
        matrix: [0; 9],
        bytes,
        device_bounds: skia_safe::IRect::new_empty(),
        scene_generation: 1,
    };
    let [first, second, third] = [1, 2, 3].map(|surface| draw_on(SurfaceId(surface)));
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
            &mut *gpu.offscreen((96, 48))?,
            &mut *gpu.offscreen((96, 48))?,
            clip,
        )?;
    }
    Ok(())
}

fn compare(surface: &mut Surface, reference: &mut Surface, clip: bool) -> Result<(), TestError> {
    let (resources, failures) = (
        PaintResources::new(&FieldRoom::new(UniformRange::VULKAN_REQUIRED))?,
        Failures::default(),
    );
    let (mut backend, command, input) = setup()?;
    background(surface.canvas());
    let _save_count = surface.canvas().save();
    let _chained_canvas = surface.canvas().scale((1.25, 1.25));
    if clip {
        let _chained_canvas = surface.canvas().clip_rect(
            skia_safe::Rect::from_xywh(10.0, 10.0, 45.0, 20.0),
            None,
            false,
        );
    }
    let request = |variant| EffectRequest {
        surface: SurfaceId(0),
        id: draw_on(SurfaceId(0)),
        variant,
        command: &command,
        input: Some(&input),
        access: SurfaceAccess::Cacheable,
        painter: ShapePainter::new(&resources, &failures),
    };
    backend.render(surface.canvas(), &request(FULL_BLUR));
    assert_eq!(backend.shared.borrow().entries.len(), 1);
    let captured = pixels(surface);
    background(reference.canvas());
    let _chained_canvas = reference.canvas().scale((1.25, 1.25));
    if clip {
        let _chained_canvas = reference.canvas().clip_rect(
            skia_safe::Rect::from_xywh(10.0, 10.0, 45.0, 20.0),
            None,
            false,
        );
    }
    let DrawCommand::Backdrop { shape, effect } = &command else {
        return Err(TestError::Missing("backdrop command"));
    };
    GlassPrograms::new(&FieldRoom::new(UniformRange::VULKAN_REQUIRED))?.backdrop(
        reference.canvas(),
        &PaintResources::new(&FieldRoom::new(UniformRange::VULKAN_REQUIRED))?,
        shape,
        BlurLevel::new(effect, 0),
        SurfaceAccess::Current,
    )?;
    let expected = pixels(reference);
    let worst = captured
        .iter()
        .zip(&expected)
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .ok_or(TestError::Missing("pixels"))?;
    let differences = captured
        .iter()
        .zip(&expected)
        .enumerate()
        .filter(|(_, (a, b))| a.abs_diff(**b) > 2)
        .map(|(i, (a, b))| (i / 4 % 96, i / 4 / 96, i % 4, a.abs_diff(*b)))
        .take(12)
        .collect::<Vec<_>>();
    assert!(
        worst <= 2,
        "cache fill differs from ordinary glass, delta {worst}: {differences:?}"
    );
    // Paint the striped background again, without the clip, before the cached image is composited. The clip
    // applies only to the glass draw.
    let _chained_canvas = surface.canvas().restore_to_count(1);
    let _chained_canvas = surface.canvas().reset_matrix();
    background(surface.canvas());
    let _chained_canvas = surface.canvas().scale((1.25, 1.25));
    if clip {
        let _chained_canvas = surface.canvas().clip_rect(
            skia_safe::Rect::from_xywh(10.0, 10.0, 45.0, 20.0),
            None,
            false,
        );
    }
    backend.render(surface.canvas(), &request(CACHED));
    let replayed = pixels(surface);
    let worst = captured
        .iter()
        .zip(&replayed)
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .ok_or(TestError::Missing("pixels"))?;
    let differences = captured
        .iter()
        .zip(&replayed)
        .enumerate()
        .filter(|(_, (a, b))| a.abs_diff(**b) > 2)
        .map(|(i, (a, b))| (i / 4 % 96, i / 4 / 96, i % 4, *a, *b))
        .take(12)
        .collect::<Vec<_>>();
    assert!(
        worst <= 2,
        "clip {clip}, replay channel delta {worst}: {differences:?}"
    );
    Ok(())
}

#[test]
fn cached_output_rejects_new_opacity_destination_and_a_new_scene() -> Result<(), TestError> {
    let (resources, failures) = (
        PaintResources::new(&FieldRoom::new(UniformRange::VULKAN_REQUIRED))?,
        Failures::default(),
    );
    let (mut backend, command, mut input) = setup()?;
    let mut surface =
        skia_safe::surfaces::raster_n32_premul((96, 48)).ok_or(TestError::RasterSurface)?;
    background(surface.canvas());
    backend.render(
        surface.canvas(),
        &EffectRequest {
            surface: SurfaceId(0),
            id: draw_on(SurfaceId(0)),
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
            surface: SurfaceId(0),
            id: draw_on(SurfaceId(0)),
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
            surface: SurfaceId(0),
            id: draw_on(SurfaceId(0)),
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
        PaintResources::new(&FieldRoom::new(UniformRange::VULKAN_REQUIRED))?,
        Failures::default(),
    );
    let (mut backend, command, input) = setup()?;
    let mut first =
        skia_safe::surfaces::raster_n32_premul((96, 48)).ok_or(TestError::RasterSurface)?;
    background(first.canvas());
    backend.render(
        first.canvas(),
        &EffectRequest {
            surface: SurfaceId(11),
            id: draw_on(SurfaceId(11)),
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
            surface: SurfaceId(21),
            id: draw_on(SurfaceId(21)),
            variant: CACHED,
            command: &command,
            input: Some(&input),
            access: SurfaceAccess::Cacheable,
            painter: ShapePainter::new(&resources, &failures),
        },
    );
    assert_eq!(pixels(&mut second), before);
    backend.forget_surface(SurfaceId(21));
    assert_eq!(backend.shared.borrow().entries.len(), 1);
    backend.forget_surface(SurfaceId(11));
    assert!(backend.shared.borrow().entries.is_empty());
    Ok(())
}

/// One column more than fits in the 16384 bytes of uniforms that Vulkan requires every device to support, at 16
/// bytes for each `float4` column. The last two shapes need more columns than that: a contour with one segment in
/// each column and a union with one member in every two columns.
const PAST_REQUIRED_COLUMNS: u16 = 16384 / 16 + 1;

fn shapes() -> Result<[Shape; 5], TestError> {
    let pill = |x| {
        RoundedRect::new(
            Rect::new(x, 8.dp(), 40.dp(), 32.dp()),
            Corners::all(16.dp()),
        )
        .into()
    };
    Ok([
        RoundedRect::new(
            Rect::new(10.dp(), 6.dp(), 76.dp(), 36.dp()),
            Corners::new(18.dp(), 0.dp(), 6.dp(), 12.dp()),
        )
        .into(),
        union([pill(6.dp()), pill(50.dp())], 12.dp())?,
        ellipse(12, Rect::new(12.dp(), 6.dp(), 72.dp(), 36.dp()))?,
        ellipse(
            PAST_REQUIRED_COLUMNS,
            Rect::new(12.dp(), 6.dp(), 72.dp(), 36.dp()),
        )?,
        union(
            (0..=PAST_REQUIRED_COLUMNS / 2).map(|i| {
                RoundedRect::new(
                    Rect::new(
                        6.dp() + 9.5.dp() * f64::from(i % 9),
                        10.dp(),
                        8.dp(),
                        28.dp(),
                    ),
                    Corners::all(4.dp()),
                )
                .into()
            }),
            6.dp(),
        )?,
    ])
}

fn glass_pixels(
    shape: &Shape,
    refraction: Dp,
    destination: Destination,
) -> Result<Vec<f32>, TestError> {
    let glass = lavashell_core::effects::Glass {
        refraction: NonNegativeDp::try_new(refraction)?,
        dispersion: NonNegativeDp::ZERO,
        rim: NonNegative::ZERO,
    };
    let effect = Backdrop::glass(Dp::ZERO, glass)?;
    let mut s = crate::effects_tests::surface()?;
    crate::effects_tests::draw(&mut s, |d| {
        stripes(d);
        if destination == Destination::Layer {
            d.commands.push(DrawCommand::Opacity(0.5));
        }
        d.material(
            shape.clone(),
            &Material::new(Color::TRANSPARENT).backdrop(effect),
        );
        if destination == Destination::Layer {
            d.restore();
        }
    })?;
    Ok((0..48)
        .flat_map(|y| (0..96).map(move |x| (x, y)))
        .map(|(x, y)| crate::effects_tests::sample(&mut s, x, y)[0])
        .collect())
}

fn changed(shape: &Shape, plain: &[f32], glass: &[f32]) -> Result<(usize, usize), TestError> {
    let mut inside = 0;
    let mut outside = 0;
    for (i, (a, b)) in plain.iter().zip(glass).enumerate() {
        let (x, y) = (u16::try_from(i % 96)?, u16::try_from(i / 96)?);
        let p = Point::new(Dp::new(f64::from(x) + 0.5), Dp::new(f64::from(y) + 0.5));
        if (a - b).abs() > 0.01 {
            if shape.distance(p) < 0.0 {
                inside += 1;
            } else if shape.distance(p) > 1.0 {
                outside += 1;
            }
        }
    }
    Ok((inside, outside))
}

#[test]
fn glass_refracts_inside_every_shape_and_nowhere_outside() -> Result<(), TestError> {
    for shape in shapes()? {
        let plain = glass_pixels(&shape, 0.dp(), Destination::Surface)?;
        let glass = glass_pixels(&shape, 9.dp(), Destination::Surface)?;
        let (inside, outside) = changed(&shape, &plain, &glass)?;
        assert!(inside > 100, "{shape:?}: {inside} refracted pixels");
        assert_eq!(outside, 0, "{shape:?}");
    }
    Ok(())
}

#[test]
fn glass_keeps_its_lens_in_layers() -> Result<(), TestError> {
    let [rounded, union, contour, ..] = shapes()?;
    for shape in [rounded, union, contour] {
        let plain = glass_pixels(&shape, 0.dp(), Destination::Layer)?;
        let glass = glass_pixels(&shape, 9.dp(), Destination::Layer)?;
        let (inside, outside) = changed(&shape, &plain, &glass)?;
        assert!(inside > 100, "{shape:?}: {inside} refracted pixels");
        assert_eq!(outside, 0, "{shape:?}");
    }
    Ok(())
}

#[test]
fn glass_that_cannot_read_the_surface_for_its_lens_blurs_and_reports_it() -> Result<(), TestError> {
    let [.., long_contour, many_members] = shapes()?;
    let glass = GlassPrograms::new(&FieldRoom::new(UniformRange::VULKAN_REQUIRED))?;
    let resources = PaintResources::new(&FieldRoom::new(UniformRange::VULKAN_REQUIRED))?;
    for shape in [long_contour, many_members] {
        let mut plain = crate::effects_tests::surface()?;
        background(plain.canvas());
        let mut surface = crate::effects_tests::surface()?;
        background(surface.canvas());
        let lens = lavashell_core::effects::Glass {
            refraction: constant!(NonNegativeDp::try_new(Dp::new(9.0))),
            dispersion: NonNegativeDp::ZERO,
            rim: NonNegative::ZERO,
        };
        let drawn = glass.backdrop(
            surface.canvas(),
            &resources,
            &shape,
            BlurLevel::new(&Backdrop::glass(4.dp(), lens)?, 0),
            SurfaceAccess::Unreadable,
        );
        assert_eq!(
            drawn.map_err(|failed| failed.failure()),
            Err(DrawFailure::LensWithoutSurface),
            "{shape:?}"
        );
        assert_ne!(
            pixels(&mut plain),
            pixels(&mut surface),
            "{shape:?} still blurs"
        );
    }
    Ok(())
}
