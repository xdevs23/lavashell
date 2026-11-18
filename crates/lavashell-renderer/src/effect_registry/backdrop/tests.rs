// SPDX-License-Identifier: GPL-3.0-only
use super::cache_checks::{assert_cached_glass_matches_ordinary_glass, background, draw_on, setup};
use super::*;
use crate::effect_registry::{ShapePainter, SurfaceAccess};
use crate::failure::Failures;
use crate::resources::PaintResources;
use crate::test_error::TestError;
use crate::test_surfaces::{EFFECT_SURFACE, pixels};
use crate::{FieldRoom, UniformRange};
use skia_safe::Surface;

fn n32_effect_surface() -> Result<Surface, TestError> {
    skia_safe::surfaces::raster_n32_premul(EFFECT_SURFACE.dimensions())
        .ok_or(TestError::RasterSurface)
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
    let glass = GlassPrograms::new(&FieldRoom::new(UniformRange::WITHOUT_DEVICE)?)?;
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
    assert_cached_glass_matches_ordinary_glass(
        &mut n32_effect_surface()?,
        &mut n32_effect_surface()?,
        true,
    )
}

#[test]
fn cached_output_rejects_new_opacity_destination_and_a_new_scene() -> Result<(), TestError> {
    let (resources, failures) = (
        PaintResources::new(
            &FieldRoom::new(UniformRange::WITHOUT_DEVICE)?,
            Failures::default(),
        )?,
        Failures::default(),
    );
    let (mut backend, command, mut input) = setup()?;
    let mut surface = n32_effect_surface()?;
    background(surface.canvas())?;
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
    background(surface.canvas())?;
    let _restored_when_the_surface_drops = surface.canvas().save_layer_alpha_f(None, 0.5);
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
    let _canvas_returned_for_chaining = surface.canvas().restore();
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
            &FieldRoom::new(UniformRange::WITHOUT_DEVICE)?,
            Failures::default(),
        )?,
        Failures::default(),
    );
    let (mut backend, command, input) = setup()?;
    let mut first = n32_effect_surface()?;
    background(first.canvas())?;
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
    let mut second = n32_effect_surface()?;
    let _canvas_returned_for_chaining = second.canvas().clear(skia_safe::Color::RED);
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
