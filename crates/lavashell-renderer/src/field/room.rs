// SPDX-License-Identifier: GPL-3.0-only

use super::program::constants;
use super::sksl::{self, Declaration};
use crate::{Error, SkslDiagnostic};
use skia_safe::RuntimeEffect;
use skia_safe::runtime_effect::Uniform;
use skia_safe::runtime_effect::uniform::Type;
use std::num::TryFromIntError;

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UniformRange(u32);

impl UniformRange {
    // https://registry.khronos.org/vulkan/specs/latest/html/vkspec.html#limits-minmax
    pub const VULKAN_REQUIRED: Self = Self(16384);

    #[must_use]
    pub fn of_device(max_uniform_buffer_range: u32) -> Self {
        Self(max_uniform_buffer_range)
    }
}

pub(super) const COLUMN_FLOATS: usize = 4;

const VECTOR_BYTES: usize = COLUMN_FLOATS * size_of::<f32>();

// https://github.com/rust-skia/skia/blob/m153-0.101.2/src/gpu/ganesh/glsl/GrGLSLProgramBuilder.cpp
const GANESH_DRAW_UNIFORMS: [Type; 3] = [Type::Float4, Type::Float2, Type::Float4];

// https://github.com/rust-skia/skia/blob/m153-0.101.2/src/gpu/ganesh/effects/GrMatrixEffect.cpp
// https://github.com/rust-skia/skia/blob/m153-0.101.2/src/gpu/ganesh/effects/GrTextureEffect.cpp
const GANESH_IMAGE_UNIFORMS: [Type; 5] = [
    Type::Float3x3,
    Type::Float4,
    Type::Float4,
    Type::Float4,
    Type::Float2,
];

#[derive(Clone, Copy, Debug)]
pub struct FieldRoom {
    buffer: usize,
    globals: usize,
}

impl FieldRoom {
    pub(crate) fn new(range: UniformRange) -> Result<Self, Error> {
        let range = usize::try_from(range.0).map_err(Error::UniformRangeSize)?;
        let buffer = largest_buffer_within(range);
        Ok(Self {
            buffer,
            globals: compiled_floats(buffer / size_of::<f32>())?,
        })
    }

    pub(super) fn beside(self, effect: &RuntimeEffect) -> Result<Self, TryFromIntError> {
        Ok(Self {
            buffer: self.buffer.saturating_sub(block_bytes(effect.uniforms())?),
            ..self
        })
    }

    pub(super) fn columns(
        self,
        effect: &RuntimeEffect,
        step: usize,
        images: usize,
    ) -> Result<usize, TryFromIntError> {
        let array_bytes = step * VECTOR_BYTES;
        let ganesh =
            types_bytes(&GANESH_DRAW_UNIFORMS) + images * types_bytes(&GANESH_IMAGE_UNIFORMS);
        let fixed_bytes = block_bytes(effect.uniforms())?.saturating_sub(array_bytes) + ganesh;
        let floats = effect.uniform_size() / size_of::<f32>();
        let fixed_floats = floats.saturating_sub(step * COLUMN_FLOATS);
        let by_bytes = self.buffer.saturating_sub(fixed_bytes) / VECTOR_BYTES;
        let by_slots = self.globals.saturating_sub(fixed_floats) / COLUMN_FLOATS;
        let most = by_bytes.min(by_slots);
        Ok(most.checked_div(step).map_or(0, |steps| steps * step))
    }
}

fn largest_buffer_within(range: usize) -> usize {
    range.checked_ilog2().map_or(0, |log| {
        let power = 1 << log;
        let midpoint = power + power / 2;
        if midpoint <= range { midpoint } else { power }
    })
}

fn compiled_floats(wanted: usize) -> Result<usize, Error> {
    let _smallest_probe = probe(1).map_err(|source| Error::CompileProgram {
        program: "uniform probe".to_owned(),
        source,
    })?;
    let compiles = |floats| probe(floats).is_ok();
    if compiles(wanted) {
        return Ok(wanted);
    }
    let (mut fits, mut exceeds) = (0, wanted);
    while exceeds - fits > 1 {
        let middle = fits + (exceeds - fits) / 2;
        if compiles(middle) {
            fits = middle;
        } else {
            exceeds = middle;
        }
    }
    Ok(fits)
}

fn probe(floats: usize) -> Result<RuntimeEffect, SkslDiagnostic> {
    let probe = Declaration::UniformArray {
        name: "probe",
        ty: sksl::Type::Float,
        length: floats,
    };
    let declarations = [constants(0), vec![probe]].concat();
    let source = sksl::render(&declarations) + "half4 main(float2 p) { return half4(probe[0]); }";
    RuntimeEffect::make_for_shader(source, None).map_err(SkslDiagnostic)
}

fn block_bytes(uniforms: &[Uniform]) -> Result<usize, TryFromIntError> {
    let bytes = |uniform: &Uniform| -> Result<usize, TryFromIntError> {
        Ok(usize::try_from(uniform.count())? * type_bytes(uniform.ty()))
    };
    uniforms.iter().map(bytes).sum()
}

fn types_bytes(types: &[Type]) -> usize {
    types.iter().map(|&ty| type_bytes(ty)).sum()
}

fn type_bytes(ty: Type) -> usize {
    let vectors = match ty {
        Type::Float2x2 => 2,
        Type::Float3x3 => 3,
        Type::Float4x4 => 4,
        Type::Float
        | Type::Float2
        | Type::Float3
        | Type::Float4
        | Type::Int
        | Type::Int2
        | Type::Int3
        | Type::Int4 => 1,
    };
    vectors * VECTOR_BYTES
}
