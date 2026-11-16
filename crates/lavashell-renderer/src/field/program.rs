// SPDX-License-Identifier: GPL-3.0-only

use super::sksl::{self, Declaration};
use crate::error::{Error, SkslDiagnostic};
use crate::failure::DrawFailure;
use crate::scalar::scalar;
use lavashell_core::numbers::count_f64;
use lavashell_core::shape::Union;
use skia_safe::runtime_effect::{ChildPtr, RuntimeShaderBuilder, ShaderBuilderError};
use skia_safe::{Blender, Data, ImageFilter, RuntimeEffect, Shader, image_filters};
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::marker::PhantomData;

pub(crate) const BOX_INTERSECTION_BLEND_RADIUS: f64 = 6.0;

// https://iquilezles.org/articles/smin/
pub(crate) const SMOOTH_MIN_QUADRATIC_FACTOR: f64 = 0.25;

pub(crate) const LENS_LENGTH_PER_RADIUS: f64 = 0.6;

pub(crate) const SHORTEST_LENS_LENGTH: f64 = 10.0;

pub(crate) fn smoothing(union: &Union) -> f64 {
    union.spacing().get() / (2.0 * SMOOTH_MIN_QUADRATIC_FACTOR)
}

pub(crate) fn count_uniform(count: usize) -> Result<f64, Error> {
    let exact = count_f64(count).map_err(|source| Error::ShaderCount { count, source })?;
    let uniform = scalar(exact).map_err(|source| Error::ShaderCountRange { count, source })?;
    if f64::from(uniform).to_bits() == exact.to_bits() {
        Ok(exact)
    } else {
        Err(Error::InexactShaderCount(count))
    }
}

pub(crate) type Layout = Vec<(Cow<'static, str>, usize)>;

pub(crate) trait Uniforms {
    const COLUMN_STEP: usize = 0;

    fn layout(columns: usize, names: &mut Layout);

    fn declare(_columns: usize) -> Vec<Declaration> {
        Vec::new()
    }

    fn columns(&self) -> usize {
        0
    }

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
        let declared: BTreeMap<_, _> = (effect.uniforms().iter())
            .map(|uniform| {
                let floats = uniform.size_in_bytes() / size_of::<f32>();
                (uniform.name().to_owned(), floats)
            })
            .collect();
        let expected: BTreeMap<_, _> = (layout.iter())
            .map(|(name, floats)| (name.clone().into_owned(), *floats))
            .collect();
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

    fn checked<'f>(&self, floats: &'f [f64]) -> Result<&'f [f64], DrawFailure> {
        (floats.len() == self.floats)
            .then_some(floats)
            .ok_or(DrawFailure::ShaderRejected)
    }

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

fn largest_half() -> f64 {
    (2.0 - 2.0_f64.powi(-10)) * 2.0_f64.powi(15)
}

pub(super) fn constants(columns: usize) -> Vec<Declaration> {
    let floats = [
        (
            "BOX_INTERSECTION_BLEND_RADIUS",
            BOX_INTERSECTION_BLEND_RADIUS,
        ),
        ("SMOOTH_MIN_QUADRATIC_FACTOR", SMOOTH_MIN_QUADRATIC_FACTOR),
        ("LENS_LENGTH_PER_RADIUS", LENS_LENGTH_PER_RADIUS),
        ("SHORTEST_LENS_LENGTH", SHORTEST_LENS_LENGTH),
        ("FAR", largest_half()),
    ];
    let floats = floats.map(|(name, value)| Declaration::ConstFloat { name, value });
    let columns = Declaration::ConstInt {
        name: "COLUMNS",
        value: columns,
    };
    floats.into_iter().chain([columns]).collect()
}

fn source<U: Uniforms>(parts: &[Part], columns: usize) -> String {
    let mut source = sksl::render(&constants(columns));
    source.push_str(include_str!("prelude.sksl"));
    source.push_str(&sksl::render(&U::declare(columns)));
    for part in parts {
        source.push_str(part.0);
    }
    source
}
