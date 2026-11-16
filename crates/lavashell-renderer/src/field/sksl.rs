// SPDX-License-Identifier: GPL-3.0-only

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

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FoldedMatrix {
    pub name: String,
    pub first: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Declaration {
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

pub(crate) fn render(declarations: &[Declaration]) -> String {
    declarations.iter().map(ToString::to_string).collect()
}
