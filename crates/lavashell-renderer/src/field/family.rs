// SPDX-License-Identifier: GPL-3.0-only

use super::program::{Children, Compile, Part, Program, Uniforms};
use super::room::FieldRoom;
use crate::Error;
use crate::failure::{DrawFailure, FailedDraw};
use std::cell::OnceCell;
use std::iter;
use std::rc::Rc;

#[cfg(test)]
mod tests;

#[derive(Clone, Copy)]
pub(crate) enum Pass<'r> {
    Own(&'r FieldRoom),
    Shared,
}

pub(crate) struct Family<U, C = ()> {
    program: String,
    parts: Vec<Part>,
    compile: Compile,
    sizes: Vec<Size<U, C>>,
}

struct Size<U, C> {
    columns: usize,
    program: OnceCell<Result<Program<U, C>, Rc<Error>>>,
}

impl<U: Uniforms, C: Children> Family<U, C> {
    pub fn new(program: &str, parts: &[Part], compile: Compile, pass: Pass) -> Result<Self, Error> {
        let step = U::COLUMN_STEP;
        let shortest = Program::compiled(program, parts, step, compile)?;
        let most =
            match pass {
                Pass::Own(room) => (room.columns(shortest.effect(), step, C::NAMES.len()))
                    .map_err(|source| Error::UniformCount {
                        program: program.to_owned(),
                        source,
                    })?,
                Pass::Shared => 0,
            };
        if most < step {
            return Err(Error::NoUniformRoom {
                program: program.to_owned(),
            });
        }
        let lengths = iter::successors(Some(step), |&columns| {
            (columns < most).then(|| (2 * columns).min(most))
        });
        let shortest = Size {
            columns: step,
            program: OnceCell::from(Ok(shortest)),
        };
        let longer = lengths.skip(1).map(|columns| Size {
            columns,
            program: OnceCell::new(),
        });
        let sizes = iter::once(shortest).chain(longer).collect();
        Ok(Self {
            program: program.to_owned(),
            parts: parts.to_vec(),
            compile,
            sizes,
        })
    }

    pub fn most(&self) -> usize {
        self.sizes.last().map_or(0, |size| size.columns)
    }

    pub fn holding(&self, columns: usize) -> Result<&Program<U, C>, FailedDraw> {
        let size = (self.sizes.iter())
            .find(|size| size.columns >= columns)
            .ok_or(DrawFailure::ShaderRejected)?;
        let program = size.program.get_or_init(|| {
            Program::compiled(&self.program, &self.parts, size.columns, self.compile)
                .map_err(Rc::new)
        });
        program
            .as_ref()
            .map_err(|cause| FailedDraw::caused(DrawFailure::ShaderRejected, Rc::clone(cause)))
    }
}
