// SPDX-License-Identifier: GPL-3.0-only
use super::parent_copy_checks::{
    assert_backdrop_reads_only_isolating_layers, assert_backdrop_reads_through,
    assert_layer_bounds_cut_the_backdrop,
};
use crate::test_error::TestError;
use crate::test_surfaces::EFFECT_SURFACE;

#[test]
fn a_backdrop_reads_through_only_a_source_over_effect_layer() -> Result<(), TestError> {
    let mut surface = EFFECT_SURFACE.raster()?;
    assert_backdrop_reads_through(&mut surface)?;
    assert_backdrop_reads_only_isolating_layers(&mut surface)
}

#[test]
fn the_bounds_of_an_effect_layer_cut_a_backdrop_that_reads_through_it() -> Result<(), TestError> {
    assert_layer_bounds_cut_the_backdrop(&mut EFFECT_SURFACE.raster()?)
}
