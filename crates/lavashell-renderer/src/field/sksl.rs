// SPDX-License-Identifier: GPL-3.0-only
//! The declarations that the renderer generates into the `SkSL` of a field program, each rendered in one form.

use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Type {
    Float,
    Float3,
    Float4,
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Float => "float",
            Self::Float3 => "float3",
            Self::Float4 => "float4",
        })
    }
}

/// A `float4x4` uniform of geometry columns that a fold passes its value through, and the index of its first column.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FoldedMatrix {
    pub name: String,
    pub first: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Declaration {
    /// A float constant. The `SkSL` compiler rounds the literal to a `float`.
    ConstFloat {
        name: &'static str,
        value: f64,
    },
    ConstInt {
        name: &'static str,
        value: usize,
    },
    UniformMatrix {
        name: String,
    },
    UniformArray {
        name: &'static str,
        ty: Type,
        length: usize,
    },
    /// The function `function`, which passes `value` through `step(value, p, exact, columns, first)` for every
    /// matrix in `matrices`. A declaration of `step` comes before the function. The parts of the program define
    /// `step`.
    Fold {
        function: &'static str,
        value: Type,
        step: &'static str,
        matrices: Vec<FoldedMatrix>,
    },
}

impl fmt::Display for Declaration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ConstFloat { name, value } => writeln!(f, "const float {name} = {value:?};"),
            Self::ConstInt { name, value } => writeln!(f, "const int {name} = {value};"),
            Self::UniformMatrix { name } => writeln!(f, "uniform float4x4 {name};"),
            Self::UniformArray { name, ty, length } => {
                writeln!(f, "uniform {ty} {name}[{length}];")
            }
            Self::Fold {
                function,
                value,
                step,
                matrices,
            } => {
                let parameters = "float2 p, float2 exact";
                writeln!(
                    f,
                    "{value} {step}({value} value, {parameters}, float4x4 columns, float first);"
                )?;
                writeln!(f, "{value} {function}({value} value, {parameters}) {{")?;
                for FoldedMatrix { name, first } in matrices {
                    writeln!(
                        f,
                        "    value = {step}(value, p, exact, {name}, float({first}));"
                    )?;
                }
                writeln!(f, "    return value;\n}}")
            }
        }
    }
}

/// The `SkSL` of `declarations`, one after another.
pub(crate) fn render(declarations: &[Declaration]) -> String {
    declarations.iter().map(ToString::to_string).collect()
}
