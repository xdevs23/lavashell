// SPDX-License-Identifier: GPL-3.0-only
use super::cache_checks::draw_on;
use super::glass::GlassPrograms;
use super::{Backend, Mode, Shared};
use crate::effect_checks::{draw, stripes};
use crate::effect_registry::{
    EffectBackend, EffectInput, EffectRequest, ShapePainter, SurfaceAccess,
};
use crate::failure::Failures;
use crate::field::test_shapes::Distance;
use crate::resources::PaintResources;
use crate::test_error::TestError;
use crate::test_surfaces::{EFFECT_SURFACE, Pixels, channel_differences, pixels};
use crate::{FieldRoom, UniformRange};
use lavashell_core::effects::Glass;
use lavashell_core::prelude::*;
use lavashell_core::surface::SurfaceId;
use skia_safe::{IRect, Matrix, Surface};
use std::{cell::RefCell, ops::DerefMut, rc::Rc};

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

pub(super) fn effects() -> [Backdrop; 2] {
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

pub(super) fn backend(shared: &Rc<RefCell<Shared>>) -> Backend {
    Backend {
        shared: shared.clone(),
    }
}

pub(super) fn shared() -> Result<Rc<RefCell<Shared>>, TestError> {
    let glass = GlassPrograms::new(&FieldRoom::new(UniformRange::VULKAN_REQUIRED)?)?;
    Ok(Rc::new(RefCell::new(Shared::new(glass))))
}

fn input() -> EffectInput {
    EffectInput {
        scene_generation: 1,
        cache_allowed: true,
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Placement {
    matrix: Matrix,
    clip: IRect,
}

pub(super) fn placements() -> Result<[Placement; 3], TestError> {
    Ok([
        Placement {
            matrix: Matrix::new_identity(),
            clip: IRect::from_size(EFFECT_SURFACE.dimensions()),
        },
        Placement {
            matrix: Matrix::scale((1.25, 1.25)),
            clip: IRect::from_xywh(12, 4, 60, 36),
        },
        Placement {
            matrix: Matrix::rotate_deg_pivot(6.0, crate::paint::skpoint(EFFECT_SURFACE.center())?),
            clip: IRect::from_xywh(4, 2, 80, 40),
        },
    ])
}

pub(super) fn background(s: &mut Surface) -> Result<&mut Surface, TestError> {
    draw(s, stripes)?;
    Ok(s)
}

#[derive(Clone, Copy)]
pub(super) struct Drawn {
    pub(super) placement: Placement,
    pub(super) lens_x: Dp,
    pub(super) effect: Backdrop,
    pub(super) access: SurfaceAccess,
    pub(super) mode: Mode,
}

pub(super) fn render(
    backend: &mut Backend,
    s: &mut Surface,
    drawn: Drawn,
) -> Result<Pixels<[u8; 4]>, TestError> {
    let Drawn {
        placement,
        lens_x,
        effect,
        access,
        mode,
    } = drawn;
    let (resources, failures) = (
        PaintResources::new(
            &FieldRoom::new(UniformRange::VULKAN_REQUIRED)?,
            Failures::default(),
        )?,
        Failures::default(),
    );
    let s = background(s)?;
    let _canvas_returned_for_chaining = s.canvas().clip_irect(placement.clip, None);
    let _canvas_returned_for_chaining = s.canvas().concat(&placement.matrix);
    let command = DrawCommand::Backdrop {
        shape: card().minus(lens(lens_x)),
        effect,
    };
    let input = input();
    backend.render(
        s.canvas(),
        &EffectRequest {
            surface: SurfaceId::numbered(0),
            id: draw_on(SurfaceId::numbered(0)),
            variant: mode.variant(),
            command: &command,
            input: Some(&input),
            access,
            painter: ShapePainter::new(&resources, &failures),
        },
    );
    Ok(pixels(s))
}

fn assert_hole(
    drawn: &Pixels<[u8; 4]>,
    stripes: &Pixels<[u8; 4]>,
    placement: Placement,
    lens_x: Dp,
    label: &str,
) -> Result<(), TestError> {
    let local = placement
        .matrix
        .invert()
        .ok_or(TestError::Missing("inverse of the matrix"))?;
    let clip = placement.clip;
    for ((x, y), &(drawn, stripe)) in drawn.zipped(stripes)?.positioned()? {
        let p = local.map_point((f32::from(x) + 0.5, f32::from(y) + 0.5));
        let p = Point::new(crate::paint::dp_of(p.x), crate::paint::dp_of(p.y));
        let clipped = !((clip.left..clip.right).contains(&i32::from(x))
            && (clip.top..clip.bottom).contains(&i32::from(y)));
        let (into_lens, into_card) = (-lens(lens_x).distance(p), -card().distance(p));
        if clipped || into_lens > 1.0 || into_card < -1.0 {
            assert_eq!(drawn, stripe, "{label}: changed at {x}, {y}");
        } else if into_lens < -1.0 && into_card > 1.0 {
            let change = channel_differences(drawn, stripe).max();
            assert!(change > Some(20), "{label}: sharp at {x}, {y}");
        }
    }
    Ok(())
}

pub fn every_blurring_variant_leaves_the_lens_sharp_on<
    S: DerefMut<Target = Surface>,
    E: From<TestError>,
>(
    surface: &mut impl FnMut() -> Result<S, E>,
) -> Result<(), E> {
    let cache = shared()?;
    let stripes = pixels(background(&mut *surface()?)?);
    for placement in placements()? {
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

pub fn the_cached_variant_replays_its_capture_for_a_moving_lens_on<
    S: DerefMut<Target = Surface>,
    E: From<TestError>,
>(
    surface: &mut impl FnMut() -> Result<S, E>,
) -> Result<(), E> {
    let stripes = pixels(background(&mut *surface()?)?);
    for placement in placements()?.into_iter().take(2) {
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
            let captured = (cache
                .borrow()
                .entries
                .peek(&draw_on(SurfaceId::numbered(0))))
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
                let worst = replayed.largest_channel_difference(&direct)?;
                assert!(worst <= 2, "{label}: {worst}");
                let entries = &cache.borrow().entries;
                assert_eq!(entries.len(), 1);
                let entry = entries
                    .peek(&draw_on(SurfaceId::numbered(0)))
                    .ok_or(TestError::Missing("cache entry"))?;
                assert_eq!(entry.image.unique_id(), captured);
                assert_eq!(entry.scene_generation, 1);
            }
        }
    }
    Ok(())
}

pub fn a_frosted_and_dimmed_card_shows_the_lens_sharp_on<
    S: DerefMut<Target = Surface>,
    E: From<TestError>,
>(
    surface: &mut impl FnMut() -> Result<S, E>,
) -> Result<(), E> {
    let shape = card().minus(lens(30.dp()));
    let plain = pixels(background(&mut *surface()?)?);
    let mut s = surface()?;
    draw(&mut s, |d| {
        stripes(d);
        d.backdrop(shape.clone(), constant!(Backdrop::blur(Dp::new(8.0))));
        d.fill(shape.clone(), Color::rgb(0).with_alpha(0.3));
    })?;
    let drawn = pixels(&mut s);
    Ok(assert_hole(
        &drawn,
        &plain,
        placements()?[0],
        30.dp(),
        "frost and dim",
    )?)
}
