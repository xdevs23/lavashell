// SPDX-License-Identifier: GPL-3.0-only
use super::tests::draw_on;
use super::*;
use crate::effect_registry::{EffectInput, ShapePainter, SurfaceAccess};
use crate::effects_tests::{draw, stripes, surface};
use crate::failure::Failures;
use crate::field::test_shapes::Distance;
use crate::test_error::TestError;
use crate::test_surfaces::pixels;
use crate::{FieldRoom, UniformRange};
use lavashell_core::effects::Glass;
use skia_safe::{IRect, Matrix, Surface};
use std::ops::DerefMut;

const WIDTH: usize = 96;

fn card() -> RoundedRect {
    RoundedRect::new(
        Rect::new(8.dp(), 6.dp(), 80.dp(), 36.dp()),
        Corners::all(10.dp()),
    )
}

fn lens(x: Dp) -> RoundedRect {
    RoundedRect::new(
        Rect::new(x, 14.dp(), 20.dp(), 20.dp()),
        Corners::all(Dp::INFINITY),
    )
}

fn effects() -> [Backdrop; 2] {
    const LENS: Glass = Glass {
        refraction: constant!(NonNegativeDp::try_new(Dp::new(2.0))),
        dispersion: constant!(NonNegativeDp::try_new(Dp::new(0.5))),
        rim: constant!(NonNegative::try_new(0.2)),
    };
    [
        constant!(Backdrop::blur(Dp::new(8.0))),
        constant!(Backdrop::glass(Dp::new(8.0), LENS)),
    ]
}

fn backend(shared: &Rc<RefCell<Shared>>) -> Backend {
    Backend {
        shared: shared.clone(),
    }
}

fn shared() -> Result<Rc<RefCell<Shared>>, TestError> {
    let glass = GlassPrograms::new(&FieldRoom::new(UniformRange::VULKAN_REQUIRED))?;
    Ok(Rc::new(RefCell::new(Shared::new(glass))))
}

fn input() -> EffectInput {
    EffectInput {
        scene_generation: 1,
        cache_allowed: true,
    }
}

#[derive(Clone, Copy, Debug)]
struct Placement {
    matrix: Matrix,
    clip: IRect,
}

/// Three placements of the card on the canvas: without a transform and clipped to the whole surface, scaled by
/// 1.25 inside a clip, and rotated by 6 degrees inside a clip.
fn placements() -> [Placement; 3] {
    [
        Placement {
            matrix: Matrix::new_identity(),
            clip: IRect::from_wh(96, 48),
        },
        Placement {
            matrix: Matrix::scale((1.25, 1.25)),
            clip: IRect::from_xywh(12, 4, 60, 36),
        },
        Placement {
            matrix: Matrix::rotate_deg_pivot(6.0, (48.0, 24.0)),
            clip: IRect::from_xywh(4, 2, 80, 40),
        },
    ]
}

fn background(s: &mut Surface) -> Result<&mut Surface, TestError> {
    draw(s, stripes)?;
    Ok(s)
}

#[derive(Clone, Copy)]
struct Drawn {
    placement: Placement,
    lens_x: Dp,
    effect: Backdrop,
    access: SurfaceAccess,
    mode: Mode,
}

fn render(backend: &mut Backend, s: &mut Surface, drawn: Drawn) -> Result<Vec<u8>, TestError> {
    let Drawn {
        placement,
        lens_x,
        effect,
        access,
        mode,
    } = drawn;
    let (resources, failures) = (
        PaintResources::new(&FieldRoom::new(UniformRange::VULKAN_REQUIRED))?,
        Failures::default(),
    );
    let s = background(s)?;
    let _chained_canvas = s.canvas().clip_irect(placement.clip, None);
    let _chained_canvas = s.canvas().concat(&placement.matrix);
    let command = DrawCommand::Backdrop {
        shape: card().minus(lens(lens_x)),
        effect,
    };
    let input = input();
    backend.render(
        s.canvas(),
        &EffectRequest {
            surface: SurfaceId(0),
            id: draw_on(SurfaceId(0)),
            variant: mode.variant(),
            command: &command,
            input: Some(&input),
            access,
            painter: ShapePainter::new(&resources, &failures),
        },
    );
    Ok(pixels(s))
}

/// Checks `drawn` against the striped background `stripes`. Pixels inside the lens hole, outside the card or
/// outside the clip show the stripes unchanged. Every pixel inside the card, outside the lens hole and inside the
/// clip is blurred.
fn assert_hole(
    drawn: &[u8],
    stripes: &[u8],
    placement: Placement,
    lens_x: Dp,
    label: &str,
) -> Result<(), TestError> {
    let local = placement
        .matrix
        .invert()
        .ok_or(TestError::Missing("inverse of the matrix"))?;
    let clip = placement.clip;
    for (i, (drawn, stripe)) in drawn.chunks(4).zip(stripes.chunks(4)).enumerate() {
        let (x, y) = (u16::try_from(i % WIDTH)?, u16::try_from(i / WIDTH)?);
        let p = local.map_point((f32::from(x) + 0.5, f32::from(y) + 0.5));
        let p = Point::new(crate::paint::dp_of(p.x), crate::paint::dp_of(p.y));
        let clipped = !((clip.left..clip.right).contains(&i32::from(x))
            && (clip.top..clip.bottom).contains(&i32::from(y)));
        let (into_lens, into_card) = (-lens(lens_x).distance(p), -card().distance(p));
        if clipped || into_lens > 1.0 || into_card < -1.0 {
            assert_eq!(drawn, stripe, "{label}: changed at {x}, {y}");
        } else if into_lens < -1.0 && into_card > 1.0 {
            let change = drawn.iter().zip(stripe).map(|(a, b)| a.abs_diff(*b)).max();
            assert!(change > Some(20), "{label}: sharp at {x}, {y}");
        }
    }
    Ok(())
}

fn every_blurring_variant_leaves_the_lens_sharp_on<S: DerefMut<Target = Surface>>(
    surface: &mut impl FnMut() -> Result<S, TestError>,
) -> Result<(), TestError> {
    let cache = shared()?;
    let stripes = pixels(background(&mut *surface()?)?);
    for placement in placements() {
        for effect in effects() {
            for level in 0..=2 {
                for access in [SurfaceAccess::Current, SurfaceAccess::Cacheable] {
                    let drawn = render(
                        &mut backend(&cache),
                        &mut *surface()?,
                        Drawn {
                            placement,
                            lens_x: 30.dp(),
                            effect,
                            access,
                            mode: Mode::Blur(level),
                        },
                    )?;
                    let label =
                        format!("{effect:?} at level {level} with {access:?} in {placement:?}");
                    assert_hole(&drawn, &stripes, placement, 30.dp(), &label)?;
                }
            }
        }
    }
    Ok(())
}

/// Captures the cached glass with the lens hole at one place and composites it again with the hole moved. The
/// result matches glass drawn directly with the moved hole. Glass under rotation is drawn directly and is not
/// cached, so only the first two placements are tested.
fn the_cached_variant_replays_its_capture_for_a_moving_lens_on<S: DerefMut<Target = Surface>>(
    surface: &mut impl FnMut() -> Result<S, TestError>,
) -> Result<(), TestError> {
    let stripes = pixels(background(&mut *surface()?)?);
    for placement in placements().into_iter().take(2) {
        for effect in effects() {
            let cache = shared()?;
            let capture_x = 16.dp();
            let capturing = render(
                &mut backend(&cache),
                &mut *surface()?,
                Drawn {
                    placement,
                    lens_x: capture_x,
                    effect,
                    access: SurfaceAccess::Cacheable,
                    mode: Mode::Blur(0),
                },
            )?;
            let label = format!("{effect:?} captured at {capture_x:?} in {placement:?}");
            assert_hole(&capturing, &stripes, placement, capture_x, &label)?;
            let captured = (cache.borrow().entries.peek(&draw_on(SurfaceId(0))))
                .ok_or(TestError::Missing("cache entry"))?
                .image
                .unique_id();
            for lens_x in [40.dp(), 58.dp()] {
                let replayed = render(
                    &mut backend(&cache),
                    &mut *surface()?,
                    Drawn {
                        placement,
                        lens_x,
                        effect,
                        access: SurfaceAccess::Cacheable,
                        mode: Mode::Cached,
                    },
                )?;
                let label = format!("{effect:?} replayed at {lens_x:?} in {placement:?}");
                assert_hole(&replayed, &stripes, placement, lens_x, &label)?;
                let direct = render(
                    &mut backend(&shared()?),
                    &mut *surface()?,
                    Drawn {
                        placement,
                        lens_x,
                        effect,
                        access: SurfaceAccess::Current,
                        mode: Mode::Blur(0),
                    },
                )?;
                let worst = replayed
                    .iter()
                    .zip(&direct)
                    .map(|(a, b)| a.abs_diff(*b))
                    .max();
                assert!(worst <= Some(2), "{label}: {worst:?}");
                let entries = &cache.borrow().entries;
                assert_eq!(entries.len(), 1);
                let entry = entries
                    .peek(&draw_on(SurfaceId(0)))
                    .ok_or(TestError::Missing("cache entry"))?;
                assert_eq!(entry.image.unique_id(), captured);
                assert_eq!(entry.scene_generation, 1);
            }
        }
    }
    Ok(())
}

fn a_frosted_and_dimmed_card_shows_the_lens_sharp_on<S: DerefMut<Target = Surface>>(
    surface: &mut impl FnMut() -> Result<S, TestError>,
) -> Result<(), TestError> {
    let shape = card().minus(lens(30.dp()));
    let plain = pixels(background(&mut *surface()?)?);
    let mut s = surface()?;
    draw(&mut s, |d| {
        stripes(d);
        d.backdrop(shape.clone(), constant!(Backdrop::blur(Dp::new(8.0))));
        d.fill(shape.clone(), Color::rgb(0).with_alpha(0.3));
    })?;
    let drawn = pixels(&mut s);
    assert_hole(&drawn, &plain, placements()[0], 30.dp(), "frost and dim")
}

#[test]
fn every_blurring_variant_leaves_the_lens_sharp() -> Result<(), TestError> {
    every_blurring_variant_leaves_the_lens_sharp_on(&mut || surface().map(Box::new))
}

#[test]
fn the_tint_variant_leaves_the_backdrop_as_it_is() -> Result<(), TestError> {
    let drawn = render(
        &mut backend(&shared()?),
        &mut surface()?,
        Drawn {
            placement: placements()[0],
            lens_x: 30.dp(),
            effect: effects()[0],
            access: SurfaceAccess::Cacheable,
            mode: Mode::Tint,
        },
    )?;
    assert_eq!(drawn, pixels(background(&mut surface()?)?));
    Ok(())
}

#[test]
fn the_cached_variant_replays_its_capture_for_a_moving_lens() -> Result<(), TestError> {
    the_cached_variant_replays_its_capture_for_a_moving_lens_on(&mut || surface().map(Box::new))
}

#[test]
fn a_frosted_and_dimmed_card_shows_the_lens_sharp() -> Result<(), TestError> {
    a_frosted_and_dimmed_card_shows_the_lens_sharp_on(&mut || surface().map(Box::new))
}

#[test]
fn vulkan_backdrops_leave_the_lens_sharp() -> Result<(), TestError> {
    let mut gpu = crate::VulkanRenderer::headless()?;
    let mut surface = || gpu.offscreen((96, 48)).map_err(TestError::Renderer);
    every_blurring_variant_leaves_the_lens_sharp_on(&mut surface)?;
    the_cached_variant_replays_its_capture_for_a_moving_lens_on(&mut surface)?;
    a_frosted_and_dimmed_card_shows_the_lens_sharp_on(&mut surface)
}
