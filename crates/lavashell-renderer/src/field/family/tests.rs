// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::SkslDiagnostic;
use crate::field::program::{Layout, shader_effect};
use crate::field::room::UniformRange;
use crate::test_error::TestError;
use skia_safe::RuntimeEffect;
use std::cell::Cell;

struct Columns(usize);

impl Uniforms for Columns {
    const COLUMN_STEP: usize = 1;

    fn layout(columns: usize, names: &mut Layout) {
        names.push(("columns".into(), 4 * columns));
    }

    fn columns(&self) -> usize {
        self.0
    }

    fn write(&self, columns: usize, floats: &mut Vec<f64>) {
        floats.extend(iter::repeat_n(0.0, 4 * columns));
    }
}

const PARTS: [Part; 1] = [Part::new(
    "uniform float4 columns[COLUMNS]; half4 main(float2 p) { return half4(columns[0]); }",
)];

fn length(program: &Program<Columns>) -> i32 {
    program.effect().uniforms()[0].count()
}

#[test]
fn a_shape_gets_the_shortest_program_that_holds_its_columns() -> Result<(), TestError> {
    let room = FieldRoom::new(UniformRange::VULKAN_REQUIRED);
    let family = Family::<Columns>::new("test", &PARTS, shader_effect, Pass::Own(&room))?;
    let most = family.most();
    assert_eq!(length(family.holding(1)?), 1);
    assert_eq!(length(family.holding(3)?), 4);
    assert_eq!(length(family.holding(most)?), i32::try_from(most)?);
    assert!(family.holding(most + 1).is_err());
    Ok(())
}

#[test]
fn a_shared_pass_leaves_no_room_for_an_array() {
    let built = Family::<Columns>::new("test", &PARTS, shader_effect, Pass::Shared);
    assert!(matches!(built, Err(Error::NoUniformRoom { .. })));
}

thread_local! {
    static COMPILED: Cell<bool> = const { Cell::new(false) };
}

/// Compiles only the first program requested on the test thread, which is the shortest program of the family. Every
/// later program fails to compile.
fn first_only(source: &str) -> Result<RuntimeEffect, SkslDiagnostic> {
    if COMPILED.replace(true) {
        Err(SkslDiagnostic("a longer program".to_owned()))
    } else {
        shader_effect(source)
    }
}

#[test]
fn a_program_that_fails_to_compile_is_rejected_with_its_cause() -> Result<(), TestError> {
    let room = FieldRoom::new(UniformRange::VULKAN_REQUIRED);
    let family = Family::<Columns>::new("test", &PARTS, first_only, Pass::Own(&room))?;
    assert_eq!(length(family.holding(1)?), 1);
    for _ in 0..2 {
        let Err(failed) = family.holding(2) else {
            return Err(TestError::Unexpected("a rejected program"));
        };
        assert_eq!(failed.failure(), DrawFailure::ShaderRejected);
        assert!(matches!(failed.cause(), Some(Error::CompileProgram { .. })));
    }
    Ok(())
}
