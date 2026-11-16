// SPDX-License-Identifier: GPL-3.0-only
use crate::error::{Error, SkslDiagnostic};
use crate::failure::DrawFailure;
use skia_safe::runtime_effect::ChildPtr;
use skia_safe::{Data, RuntimeEffect, Shader};

pub(super) struct FaceBackdrop {
    effect: RuntimeEffect,
}

impl FaceBackdrop {
    pub fn new() -> Result<Self, Error> {
        let effect = RuntimeEffect::make_for_shader(include_str!("face_backdrop.sksl"), None)
            .map_err(|diagnostic| Error::CompileSubpixelEffect {
                effect: "face backdrop",
                source: SkslDiagnostic(diagnostic),
            })?;
        Ok(Self { effect })
    }

    pub fn opposite_of(&self, drawn: Shader) -> Result<Shader, DrawFailure> {
        let children = [ChildPtr::from(drawn)];
        (self.effect)
            .make_shader(Data::new_empty(), &children, None)
            .ok_or(DrawFailure::ShaderRejected)
    }
}
