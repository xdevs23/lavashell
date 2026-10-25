// SPDX-License-Identifier: GPL-3.0-only
use super::{Color, Gamut, Model};
use crate::test_error::TestResult;
use std::mem::discriminant;

const QUARTER: [u8; 8] = [0, 0, 0, 0, 0, 0, 208, 63];
const HALF: [u8; 8] = [0, 0, 0, 0, 0, 0, 224, 63];
const THREE_QUARTERS: [u8; 8] = [0, 0, 0, 0, 0, 0, 232, 63];
const ONE: [u8; 8] = [0, 0, 0, 0, 0, 0, 240, 63];
const TWO: [u8; 8] = [0, 0, 0, 0, 0, 0, 0, 64];
const MINUS_EIGHTH: [u8; 8] = [0, 0, 0, 0, 0, 0, 192, 191];
const HALF_TURN: [u8; 8] = [0, 0, 0, 0, 0, 128, 102, 64];

/// A color of the kind of `model` and the bytes that it is stored as. The match names every model, so a new model
/// fails to compile until it has a sample.
fn sample(model: Model) -> (Color, Vec<u8>) {
    match model {
        Model::Oklch => (
            Color::oklch(0.5, 0.25, 180.0),
            [&[0][..], &HALF, &QUARTER, &HALF_TURN, &ONE].concat(),
        ),
        Model::Oklab => (
            Color::oklab(0.75, -0.125, 0.25),
            [&[1][..], &THREE_QUARTERS, &MINUS_EIGHTH, &QUARTER, &ONE].concat(),
        ),
        Model::Encoded(_) => (
            Color::encoded([0.25, 0.5, 0.75], 0.5, Gamut::DisplayP3),
            [&[2, 1][..], &QUARTER, &HALF, &THREE_QUARTERS, &HALF].concat(),
        ),
        Model::Linear(_) => (
            Color::linear([0.5, 1.0, 2.0], 0.25, Gamut::Rec2020),
            [&[3, 2][..], &HALF, &ONE, &TWO, &QUARTER].concat(),
        ),
    }
}

#[test]
fn every_color_model_writes_its_stored_bytes_and_reads_back_from_them() -> TestResult {
    for model in [
        Model::Oklch,
        Model::Oklab,
        Model::Encoded(Gamut::Srgb),
        Model::Linear(Gamut::Srgb),
    ] {
        let (color, stored) = sample(model);
        assert_eq!(discriminant(&color.model), discriminant(&model));
        assert_eq!(postcard::to_allocvec(&color)?, stored);
        assert_eq!(postcard::from_bytes::<Color>(&stored)?, color);
    }
    Ok(())
}
