// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::SkslDiagnostic;
use crate::field::program::shader_effect;
use crate::test_error::TestError;

const ARRAY: &str = "uniform float4 columns[1]; uniform float count; \
                     half4 main(float2 p) { return half4(columns[0] * count); }";

fn effect() -> Result<RuntimeEffect, SkslDiagnostic> {
    shader_effect(ARRAY)
}

#[test]
fn the_largest_buffer_is_a_power_of_two_or_the_midpoint_above_it() {
    assert_eq!(largest_buffer_within(16384), 16384);
    assert_eq!(largest_buffer_within(65535), 49152);
    assert_eq!(largest_buffer_within(100_000), 98304);
    assert_eq!(largest_buffer_within(0), 0);
}

#[test]
fn the_compiled_floats_stop_where_skia_rejects_the_globals() {
    assert_eq!(compiled_floats(64), 64);
    let most = compiled_floats(1 << 20);
    assert!(compiles(most));
    assert!(!compiles(most + 1));
}

#[test]
fn a_vector_more_of_the_buffer_holds_a_column_more() -> Result<(), TestError> {
    let room = FieldRoom {
        buffer: 4096,
        globals: usize::MAX,
    };
    let wider = FieldRoom {
        buffer: room.buffer + VECTOR_BYTES,
        ..room
    };
    let effect = effect()?;
    let columns = room.columns(&effect, 1, 0)?;
    assert_eq!(wider.columns(&effect, 1, 0)?, columns + 1);
    assert_eq!(room.columns(&effect, 4, 0)?, columns / 4 * 4);
    assert!(room.columns(&effect, 1, 1)? < columns);
    Ok(())
}

#[test]
fn the_globals_bound_the_columns_beside_the_other_uniforms() -> Result<(), TestError> {
    let room = FieldRoom {
        buffer: usize::MAX,
        globals: 1 + 10 * COLUMN_FLOATS,
    };
    assert_eq!(room.columns(&effect()?, 1, 0)?, 10);
    Ok(())
}
