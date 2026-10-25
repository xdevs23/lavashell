// SPDX-License-Identifier: GPL-3.0-only
//! Field programs built from parts of `SkSL`. The Rust type of a program names
//! its uniforms and the child shaders it samples. Building a program checks once
//! that the compiled `SkSL` declares exactly those uniforms and children.

use super::sksl::{self, Declaration};
use crate::error::{Error, SkslDiagnostic};
use crate::failure::DrawFailure;
use crate::scalar::scalar;
use lavashell_core::numbers::count_f64;
use lavashell_core::shape::Union;
use skia_safe::runtime_effect::{ChildPtr, RuntimeShaderBuilder, ShaderBuilderError};
use skia_safe::{Blender, Data, ImageFilter, RuntimeEffect, Shader, image_filters};
use std::borrow::Cow;
use std::marker::PhantomData;

/// Design token of the glass look: the band in dp over which a lens field rounds into the box that limits it.
pub(crate) const SMOOTHING: f64 = 6.0;

/// How far the smooth minimum of two equal distances lies below them, as a
/// share of the smoothing width.
pub(crate) const SMOOTH_MIN_DEPTH: f64 = 0.25;

/// Design token of the glass look: the share of the corner radius over which the lens bends light.
pub(crate) const LENS_SHARE: f64 = 0.6;

/// Design token of the glass look: the shortest length in dp over which a lens bends light, also at a sharp corner.
pub(crate) const SHORTEST_LENS: f64 = 10.0;

/// The width over which the smooth minimum blends the members of `union`.
/// Two members fuse where half of the gap between them is at most the depth of
/// the smooth minimum. At this width, every gap up to the spacing of `union` fuses.
pub(crate) fn smoothing(union: &Union) -> f64 {
    union.spacing().get() / (2.0 * SMOOTH_MIN_DEPTH)
}

/// `count` as a float for a uniform.
///
/// # Errors
/// Fails for a count that an `f32` uniform cannot represent exactly.
pub(crate) fn count_uniform(count: usize) -> Result<f64, Error> {
    let exact = count_f64(count).map_err(|source| Error::ShaderCount { count, source })?;
    let uniform = scalar(exact).map_err(|source| Error::ShaderCountRange { count, source })?;
    if f64::from(uniform).to_bits() == exact.to_bits() {
        Ok(exact)
    } else {
        Err(Error::InexactShaderCount(count))
    }
}

/// The name and the number of floats of each uniform, in the order that the uniforms are written.
pub(crate) type Layout = Vec<(Cow<'static, str>, usize)>;

/// The uniforms of a program. A program has at most one array of `float4` columns. Each program is compiled for one
/// length of that array.
pub(crate) trait Uniforms {
    /// The number of columns added from one array length to the next, or zero for uniforms without an array.
    const COLUMN_STEP: usize = 0;

    fn layout(columns: usize, names: &mut Layout);

    /// The `SkSL` declarations that change with the length of the array, for an array of `columns` columns.
    fn declare(_columns: usize) -> Vec<Declaration> {
        Vec::new()
    }

    /// The number of array columns needed by these values.
    fn columns(&self) -> usize {
        0
    }

    /// Appends these values to `floats`, with the array padded to `columns` columns. The program converts each value
    /// to the `f32` of its uniform later.
    fn write(&self, columns: usize, floats: &mut Vec<f64>);
}

impl Uniforms for () {
    fn layout(_: usize, _: &mut Layout) {}

    fn write(&self, _: usize, _: &mut Vec<f64>) {}
}

impl<A: Uniforms, B: Uniforms> Uniforms for (A, B) {
    const COLUMN_STEP: usize = if A::COLUMN_STEP > B::COLUMN_STEP {
        A::COLUMN_STEP
    } else {
        B::COLUMN_STEP
    };

    fn layout(columns: usize, names: &mut Layout) {
        A::layout(columns, names);
        B::layout(columns, names);
    }

    fn declare(columns: usize) -> Vec<Declaration> {
        [A::declare(columns), B::declare(columns)].concat()
    }

    fn columns(&self) -> usize {
        self.0.columns().max(self.1.columns())
    }

    fn write(&self, columns: usize, floats: &mut Vec<f64>) {
        self.0.write(columns, floats);
        self.1.write(columns, floats);
    }
}

macro_rules! uniform_block {
    ($(#[$meta:meta])* $name:ident { $($uniform:ident: $floats:expr),* $(,)? }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug)]
        pub(crate) struct $name {
            $(pub $uniform: [f64; $floats],)*
        }
        impl $crate::field::Uniforms for $name {
            fn layout(_: usize, names: &mut $crate::field::Layout) {
                $(names.push((stringify!($uniform).into(), $floats));)*
            }
            fn write(&self, _: usize, floats: &mut Vec<f64>) {
                $(floats.extend_from_slice(&self.$uniform);)*
            }
        }
    };
}

pub(crate) use uniform_block;

pub(crate) trait Children {
    const NAMES: &'static [&'static str];

    fn shaders(self) -> Vec<Shader>;
}

impl Children for () {
    const NAMES: &'static [&'static str] = &[];

    fn shaders(self) -> Vec<Shader> {
        Vec::new()
    }
}

macro_rules! children {
    ($(#[$meta:meta])* $name:ident { $($child:ident),* $(,)? }) => {
        $(#[$meta])*
        pub(crate) struct $name {
            $(pub $child: skia_safe::Shader,)*
        }
        impl $crate::field::Children for $name {
            const NAMES: &'static [&'static str] = &[$(stringify!($child)),*];
            fn shaders(self) -> Vec<skia_safe::Shader> {
                vec![$(self.$child),*]
            }
        }
    };
}

pub(crate) use children;

#[derive(Clone, Copy)]
pub(crate) struct Part(&'static str);

impl Part {
    pub const fn new(source: &'static str) -> Self {
        Self(source)
    }
}

/// Compiles the `SkSL` of a program into a shader or a blender.
pub(crate) type Compile = fn(&str) -> Result<RuntimeEffect, SkslDiagnostic>;

pub(crate) fn shader_effect(source: &str) -> Result<RuntimeEffect, SkslDiagnostic> {
    RuntimeEffect::make_for_shader(source, None).map_err(SkslDiagnostic)
}

pub(crate) fn blender_effect(source: &str) -> Result<RuntimeEffect, SkslDiagnostic> {
    RuntimeEffect::make_for_blender(source, None).map_err(SkslDiagnostic)
}

pub(crate) struct Program<U, C = ()> {
    compiled: Compiled,
    types: PhantomData<fn(&U, C)>,
}

struct Compiled {
    effect: RuntimeEffect,
    /// The name, byte offset and number of floats of each uniform, in the
    /// order that the uniforms are written.
    uniforms: Vec<(Cow<'static, str>, usize, usize)>,
    floats: usize,
    columns: usize,
    children: Vec<usize>,
}

impl<U: Uniforms, C: Children> Program<U, C> {
    pub fn shader(program: &str, parts: &[Part]) -> Result<Self, Error> {
        Self::compiled(program, parts, 0, shader_effect)
    }

    pub fn blender(program: &str, parts: &[Part]) -> Result<Self, Error> {
        Self::compiled(program, parts, 0, blender_effect)
    }

    /// Compiles the program from `parts` with an array of `columns` columns. Fails when the compiled program does not
    /// declare exactly the uniforms of `U` and the children of `C`.
    pub(super) fn compiled(
        program: &str,
        parts: &[Part],
        columns: usize,
        compile: Compile,
    ) -> Result<Self, Error> {
        let effect =
            compile(&source::<U>(parts, columns)).map_err(|source| Error::CompileProgram {
                program: program.to_owned(),
                source,
            })?;
        let mut layout = Layout::new();
        U::layout(columns, &mut layout);
        let declared = sorted(
            effect
                .uniforms()
                .iter()
                .map(|uniform| {
                    (
                        uniform.name().to_owned(),
                        uniform.size_in_bytes() / size_of::<f32>(),
                    )
                })
                .collect(),
        );
        let expected = sorted(
            layout
                .iter()
                .map(|(name, floats)| (name.clone().into_owned(), *floats))
                .collect(),
        );
        if declared != expected {
            return Err(Error::ProgramUniforms {
                program: program.to_owned(),
                declared,
                expected,
            });
        }
        let floats = layout.iter().map(|(_, floats)| floats).sum();
        let uniforms = (layout.into_iter())
            .map(|(name, floats)| match effect.find_uniform(&name) {
                Some(uniform) => Ok((name, uniform.offset(), floats)),
                None => Err(Error::MissingUniform {
                    program: program.to_owned(),
                    name: name.into_owned(),
                }),
            })
            .collect::<Result<_, _>>()?;
        let declared = sorted(
            effect
                .children()
                .iter()
                .map(|child| child.name().to_owned())
                .collect(),
        );
        let expected = sorted(C::NAMES.iter().map(|&name| name.to_owned()).collect());
        if declared != expected {
            return Err(Error::ProgramChildren {
                program: program.to_owned(),
                declared,
                expected,
            });
        }
        let children = (C::NAMES.iter())
            .map(|&name| match effect.find_child(name) {
                Some(child) => Ok(child.index()),
                None => Err(Error::MissingChild {
                    program: program.to_owned(),
                    name,
                }),
            })
            .collect::<Result<_, _>>()?;
        Ok(Self {
            compiled: Compiled {
                effect,
                uniforms,
                floats,
                columns,
                children,
            },
            types: PhantomData,
        })
    }

    pub(super) fn effect(&self) -> &RuntimeEffect {
        &self.compiled.effect
    }

    pub fn evaluate(&self, uniforms: &U, children: C) -> Result<Shader, DrawFailure> {
        self.compiled
            .evaluate(&self.compiled.floats(uniforms), children.shaders())
    }
}

impl<U: Uniforms> Program<U> {
    pub fn blend(&self, uniforms: &U) -> Result<Blender, DrawFailure> {
        self.compiled
            .effect
            .make_blender(self.compiled.data(&self.compiled.floats(uniforms))?, None)
            .ok_or(DrawFailure::ShaderRejected)
    }
}

impl<A: Uniforms, B: Uniforms, C: Children> Program<(A, B), C> {
    pub fn bind(&self, first: &A) -> Bound<'_, B, C> {
        Bound {
            compiled: &self.compiled,
            first: self.compiled.floats(first),
            types: PhantomData,
        }
    }
}

pub(crate) struct Bound<'p, B, C = ()> {
    compiled: &'p Compiled,
    first: Vec<f64>,
    types: PhantomData<fn(&B, C)>,
}

impl<B: Uniforms, C: Children> Bound<'_, B, C> {
    pub fn evaluate(&self, rest: &B, children: C) -> Result<Shader, DrawFailure> {
        self.compiled
            .evaluate(&self.joined(rest), children.shaders())
    }

    /// An image filter that runs the program with `input` as its only child. `reach` is the farthest distance in dp
    /// from a pixel at which the program samples `input`.
    pub fn filter(
        &self,
        rest: &B,
        reach: f64,
        input: Option<ImageFilter>,
    ) -> Result<ImageFilter, DrawFailure> {
        let child = const {
            assert!(
                C::NAMES.len() == 1,
                "a filter passes its input as the only child"
            );
            C::NAMES[0]
        };
        let mut builder = RuntimeShaderBuilder::new(self.compiled.effect.clone());
        let floats = self.joined(rest);
        let mut floats = self.compiled.checked(&floats)?;
        for (name, _, size) in &self.compiled.uniforms {
            let (uniform, others) = floats.split_at(*size);
            let uniform = (uniform.iter())
                .map(|value| scalar(*value))
                .collect::<Result<Vec<_>, _>>()?;
            (builder.set_uniform_float(name, &uniform)).map_err(|error| match error {
                ShaderBuilderError::UniformSizeNotSupported => DrawFailure::ShaderRejected,
            })?;
            floats = others;
        }
        image_filters::runtime_shader_with_options(&builder, scalar(reach)?, child, input, false)
            .ok_or(DrawFailure::ShaderRejected)
    }

    fn joined(&self, rest: &B) -> Vec<f64> {
        let mut floats = self.first.clone();
        rest.write(self.compiled.columns, &mut floats);
        floats
    }
}

impl Compiled {
    fn floats(&self, uniforms: &impl Uniforms) -> Vec<f64> {
        let mut floats = Vec::new();
        uniforms.write(self.columns, &mut floats);
        floats
    }

    /// Returns `floats` when their number equals the number of floats in all uniforms of the program. Each uniform is
    /// filled from the next floats in order.
    fn checked<'f>(&self, floats: &'f [f64]) -> Result<&'f [f64], DrawFailure> {
        (floats.len() == self.floats)
            .then_some(floats)
            .ok_or(DrawFailure::ShaderRejected)
    }

    /// The uniform data of the program, with each `f32` of each uniform converted from the next value of `floats`.
    fn data(&self, floats: &[f64]) -> Result<Data, DrawFailure> {
        let mut bytes = vec![0; self.effect.uniform_size()];
        let mut floats = self.checked(floats)?.iter();
        for &(_, offset, size) in &self.uniforms {
            let uniform = &mut bytes[offset..offset + size * size_of::<f32>()];
            let slots = uniform.as_chunks_mut::<{ size_of::<f32>() }>().0;
            for (slot, value) in slots.iter_mut().zip(floats.by_ref()) {
                slot.copy_from_slice(&scalar(*value)?.to_ne_bytes());
            }
        }
        Ok(Data::new_copy(&bytes))
    }

    fn evaluate(&self, floats: &[f64], shaders: Vec<Shader>) -> Result<Shader, DrawFailure> {
        let mut children = self.children.iter().zip(shaders).collect::<Vec<_>>();
        children.sort_by_key(|(index, _)| **index);
        let children = children
            .into_iter()
            .map(|(_, shader)| ChildPtr::from(shader))
            .collect::<Vec<_>>();
        self.effect
            .make_shader(self.data(floats)?, &children, None)
            .ok_or(DrawFailure::ShaderRejected)
    }
}

fn sorted<T: Ord>(mut values: Vec<T>) -> Vec<T> {
    values.sort();
    values
}

/// The largest half float: eleven significant bits at the largest exponent, fifteen.
fn largest_half() -> f64 {
    (2.0 - 2.0_f64.powi(-10)) * 2.0_f64.powi(15)
}

/// The constants read by the shared prelude and by the parts. `FAR` is farther than any outline and still fits into
/// the half floats of a field texture. The distance to an outline without segments is `FAR`. `COLUMNS` is the length
/// of the uniform array of the program.
pub(super) fn constants(columns: usize) -> Vec<Declaration> {
    let floats = [
        ("SMOOTHING", SMOOTHING),
        ("SMOOTH_MIN_DEPTH", SMOOTH_MIN_DEPTH),
        ("LENS_SHARE", LENS_SHARE),
        ("SHORTEST_LENS", SHORTEST_LENS),
        ("FAR", largest_half()),
    ];
    let floats = floats.map(|(name, value)| Declaration::ConstFloat { name, value });
    let columns = Declaration::ConstInt {
        name: "COLUMNS",
        value: columns,
    };
    floats.into_iter().chain([columns]).collect()
}

/// The `SkSL` of a program: the constants, the prelude, what `U` declares for the length of its array, then the
/// parts in order.
fn source<U: Uniforms>(parts: &[Part], columns: usize) -> String {
    let mut source = sksl::render(&constants(columns));
    source.push_str(include_str!("prelude.sksl"));
    source.push_str(&sksl::render(&U::declare(columns)));
    for part in parts {
        source.push_str(part.0);
    }
    source
}
