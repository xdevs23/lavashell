// SPDX-License-Identifier: GPL-3.0-only
//! How many uniforms a field program can fill in one draw. Ganesh, the GPU backend of Skia, binds all uniforms of a
//! draw as one std140 uniform buffer, together with the uniforms that Ganesh adds itself. The size of that buffer is
//! limited by the device. The `SkSL` compiler also limits how many floats the global variables of a program contain.

use super::program::constants;
use super::sksl::{self, Declaration};
use skia_safe::RuntimeEffect;
use skia_safe::runtime_effect::Uniform;
use skia_safe::runtime_effect::uniform::Type;
use std::num::TryFromIntError;

#[cfg(test)]
mod tests;

/// The most bytes of one uniform buffer that a shader on a device can read, reported by Vulkan as
/// `maxUniformBufferRange`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UniformRange(u32);

impl UniformRange {
    /// The smallest `maxUniformBufferRange` that Vulkan allows a device to report. A painter that does not know its
    /// device sizes the field programs for this range.
    pub const VULKAN_REQUIRED: Self = Self(16384);

    #[must_use]
    pub fn of_device(max_uniform_buffer_range: u32) -> Self {
        Self(max_uniform_buffer_range)
    }
}

/// The number of floats in one column of the uniform array of a field program. A column is one `float4`.
pub(super) const COLUMN_FLOATS: usize = 4;

/// The size in bytes of a `float4` in a std140 block. Every vector and every column of a matrix in such a block
/// starts at a multiple of this size.
const VECTOR_BYTES: usize = COLUMN_FLOATS * size_of::<f32>();

/// The uniforms that Ganesh adds to every draw: `sk_RTAdjust`, the flip of `sk_FragCoord` and the coordinates of
/// the copy of the destination that a blender reads, as of Skia m153:
/// <https://github.com/rust-skia/skia/blob/m153-0.101.2/src/gpu/ganesh/glsl/GrGLSLProgramBuilder.cpp>
const DRAW: [Type; 3] = [Type::Float4, Type::Float2, Type::Float4];

/// The uniforms that Ganesh adds for every image that a program samples: the matrix of `GrMatrixEffect` and the
/// border, subset, clamp and inverse dimensions of `GrTextureEffect`, as of Skia m153:
/// <https://github.com/rust-skia/skia/blob/m153-0.101.2/src/gpu/ganesh/effects/GrMatrixEffect.cpp> and
/// <https://github.com/rust-skia/skia/blob/m153-0.101.2/src/gpu/ganesh/effects/GrTextureEffect.cpp>
const IMAGE: [Type; 5] = [
    Type::Float3x3,
    Type::Float4,
    Type::Float4,
    Type::Float4,
    Type::Float2,
];

/// The uniform space that a field program can fill on a device when the program runs in a draw of its own.
#[derive(Clone, Copy, Debug)]
pub struct FieldRoom {
    /// The size in bytes of the largest uniform buffer that Ganesh allocates within the `maxUniformBufferRange` of the
    /// device.
    buffer: usize,
    /// The most floats in global variables that the `SkSL` compiler accepts in a program next to the constants
    /// declared at the start of every field program.
    globals: usize,
}

impl FieldRoom {
    pub(crate) fn new(range: UniformRange) -> Self {
        let buffer = largest_buffer_within(usize::try_from(range.0).unwrap_or(usize::MAX));
        Self {
            buffer,
            globals: compiled_floats(buffer / size_of::<f32>()),
        }
    }

    /// The uniform space left for a field program in a draw that also runs `effect`, such as a blender that combines
    /// the field with the texels drawn before it. Fails when Skia reports a negative count for a uniform of `effect`.
    pub(super) fn beside(self, effect: &RuntimeEffect) -> Result<Self, TryFromIntError> {
        Ok(Self {
            buffer: self.buffer.saturating_sub(block_bytes(effect.uniforms())?),
            ..self
        })
    }

    /// The most columns that the uniform array of a program can have, as a multiple of `step`. `effect` is the
    /// program compiled for `step` columns, and it samples `images` images. Fails when Skia reports a negative count
    /// for a uniform of `effect`.
    pub(super) fn columns(
        self,
        effect: &RuntimeEffect,
        step: usize,
        images: usize,
    ) -> Result<usize, TryFromIntError> {
        let array_bytes = step * VECTOR_BYTES;
        let ganesh = types_bytes(&DRAW) + images * types_bytes(&IMAGE);
        let fixed_bytes = block_bytes(effect.uniforms())?.saturating_sub(array_bytes) + ganesh;
        let floats = effect.uniform_size() / size_of::<f32>();
        let fixed_floats = floats.saturating_sub(step * COLUMN_FLOATS);
        let by_bytes = self.buffer.saturating_sub(fixed_bytes) / VECTOR_BYTES;
        let by_slots = self.globals.saturating_sub(fixed_floats) / COLUMN_FLOATS;
        let most = by_bytes.min(by_slots);
        Ok(most.checked_div(step).map_or(0, |steps| steps * step))
    }
}

/// The size of the largest uniform buffer within `range` that Ganesh allocates. Ganesh rounds the size of a uniform
/// buffer up to a power of two or to the midpoint between two powers (`GrResourceProvider::createBuffer`). It binds
/// the whole buffer.
fn largest_buffer_within(range: usize) -> usize {
    range.checked_ilog2().map_or(0, |log| {
        let power = 1 << log;
        let midpoint = power + power / 2;
        if midpoint <= range { midpoint } else { power }
    })
}

/// The most floats, up to `wanted`, of a uniform array that the `SkSL` compiler accepts next to the constants declared
/// at the start of every field program. Compiles probe programs to find the number.
fn compiled_floats(wanted: usize) -> usize {
    if compiles(wanted) {
        return wanted;
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
    fits
}

fn compiles(floats: usize) -> bool {
    let probe = Declaration::UniformArray {
        name: "probe",
        ty: sksl::Type::Float,
        length: floats,
    };
    let declarations = [constants(0), vec![probe]].concat();
    let source = sksl::render(&declarations) + "half4 main(float2 p) { return half4(probe[0]); }";
    RuntimeEffect::make_for_shader(source, None).is_ok()
}

/// The most bytes that `uniforms` take up in a std140 block. Fails when Skia reports a negative count for a uniform.
fn block_bytes(uniforms: &[Uniform]) -> Result<usize, TryFromIntError> {
    let bytes = |uniform: &Uniform| -> Result<usize, TryFromIntError> {
        Ok(usize::try_from(uniform.count())? * type_bytes(uniform.ty()))
    };
    uniforms.iter().map(bytes).sum()
}

fn types_bytes(types: &[Type]) -> usize {
    types.iter().map(|&ty| type_bytes(ty)).sum()
}

/// The most bytes that one value of `ty` takes up in a std140 block.
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
