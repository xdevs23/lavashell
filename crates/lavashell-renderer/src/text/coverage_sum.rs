// SPDX-License-Identifier: GPL-3.0-only
use crate::error::{Error, SkslDiagnostic};
use crate::failure::DrawFailure;
use skia_safe::runtime_effect::ChildPtr;
use skia_safe::{Data, Paint, RuntimeEffect, Shader};

pub(super) struct CoverageSum {
    effect: RuntimeEffect,
}

impl CoverageSum {
    pub fn new() -> Result<Self, Error> {
        let effect = RuntimeEffect::make_for_blender(include_str!("coverage_sum.sksl"), None)
            .map_err(|diagnostic| Error::CompileProgram {
                program: "coverage sum".to_owned(),
                source: SkslDiagnostic(diagnostic),
            })?;
        Ok(Self { effect })
    }

    pub fn paint(&self, backdrop: Shader, shares: [f32; 2]) -> Result<Paint, DrawFailure> {
        let children = [ChildPtr::from(backdrop)];
        let uniforms: Vec<u8> = shares
            .iter()
            .flat_map(|share| share.to_ne_bytes())
            .collect();
        let blender = (self.effect)
            .make_blender(Data::new_copy(&uniforms), Some(children.as_slice()))
            .ok_or(DrawFailure::ShaderRejected)?;
        let mut paint = Paint::default();
        let _paint_returned_for_chaining = paint.set_blender(blender);
        Ok(paint)
    }
}
