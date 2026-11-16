// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::Error;
use crate::field::program::count_uniform;
use lavashell_core::numbers::count_f64;

uniform_block!(Pair { a: 2 });

uniform_block!(Single { a: 1 });

uniform_block!(Renamed { b: 2 });

children!(Field { field });

#[test]
fn programs_whose_uniforms_or_children_differ_from_their_types_fail_to_build() {
    const SOURCE: &str = "uniform float2 a; half4 main(float2 p) { return half4(a.x); }";
    const SAMPLING: &str = "uniform shader field; half4 main(float2 p) { return field.eval(p); }";
    let part = [Part::new(SOURCE)];
    assert!(Program::<Pair>::shader("matching", &part).is_ok());
    let uniforms = |failure: Option<Error>| {
        assert!(matches!(failure, Some(Error::ProgramUniforms { .. })));
    };
    uniforms(Program::<Single>::shader("fewer", &part).err());
    uniforms(Program::<Renamed>::shader("renamed", &part).err());
    uniforms(Program::<()>::shader("none", &part).err());
    let sampling = [Part::new(SAMPLING)];
    assert!(Program::<(), Field>::shader("sampling", &sampling).is_ok());
    assert!(matches!(
        Program::<()>::shader("unpassed", &sampling),
        Err(Error::ProgramChildren { .. })
    ));
    assert!(matches!(
        Program::<()>::shader("broken", &[Part::new("half4 main(float2 p) { return q; }")]),
        Err(Error::CompileProgram { .. })
    ));
}

#[test]
fn a_count_is_a_uniform_float_only_where_the_float_holds_it_exactly() -> Result<(), TestError> {
    let largest_exact = 1 << f32::MANTISSA_DIGITS;
    assert_eq!(
        count_uniform(largest_exact)?.to_bits(),
        count_f64(largest_exact)?.to_bits()
    );
    let inexact = largest_exact + 1;
    assert!(matches!(
        count_uniform(inexact),
        Err(Error::InexactShaderCount(count)) if count == inexact
    ));
    Ok(())
}
