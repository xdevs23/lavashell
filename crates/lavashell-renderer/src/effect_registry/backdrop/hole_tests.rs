// SPDX-License-Identifier: GPL-3.0-only
use super::hole_checks::{
    Drawn, a_frosted_and_dimmed_card_shows_the_lens_sharp_on, backend, background, effects,
    every_blurring_variant_leaves_the_lens_sharp_on, placements, render, shared,
    the_cached_variant_replays_its_capture_for_a_moving_lens_on,
};
use super::*;
use crate::effect_registry::SurfaceAccess;
use crate::test_error::TestError;
use crate::test_surfaces::{EFFECT_SURFACE, pixels};

#[test]
fn every_blurring_variant_leaves_the_lens_sharp() -> Result<(), TestError> {
    every_blurring_variant_leaves_the_lens_sharp_on(&mut || EFFECT_SURFACE.raster().map(Box::new))
}

#[test]
fn the_tint_variant_leaves_the_backdrop_as_it_is() -> Result<(), TestError> {
    let drawn = render(
        &mut backend(&shared()?),
        &mut EFFECT_SURFACE.raster()?,
        Drawn {
            placement: placements()?[0],
            lens_x: 30.dp(),
            effect: effects()[0],
            access: SurfaceAccess::Cacheable,
            mode: Mode::Tint,
        },
    )?;
    assert_eq!(drawn, pixels(background(&mut EFFECT_SURFACE.raster()?)?));
    Ok(())
}

#[test]
fn the_cached_variant_replays_its_capture_for_a_moving_lens() -> Result<(), TestError> {
    the_cached_variant_replays_its_capture_for_a_moving_lens_on(&mut || {
        EFFECT_SURFACE.raster().map(Box::new)
    })
}

#[test]
fn a_frosted_and_dimmed_card_shows_the_lens_sharp() -> Result<(), TestError> {
    a_frosted_and_dimmed_card_shows_the_lens_sharp_on(&mut || EFFECT_SURFACE.raster().map(Box::new))
}
