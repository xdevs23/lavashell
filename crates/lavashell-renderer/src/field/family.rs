// SPDX-License-Identifier: GPL-3.0-only
//! A family of field programs compiled from one `SkSL` source, one program per length of the uniform array. A field
//! program reads every column of its array at every pixel. A shape therefore uses the shortest program whose array
//! fits its geometry.

use super::program::{Children, Compile, Part, Program, Uniforms};
use super::room::FieldRoom;
use crate::Error;
use crate::failure::{DrawFailure, FailedDraw};
use std::cell::OnceCell;
use std::iter;
use std::rc::Rc;

#[cfg(test)]
mod tests;

/// Whether the field programs of a consumer run in draws of their own or inside the draws of other programs.
#[derive(Clone, Copy)]
pub(crate) enum Pass<'r> {
    /// Each program runs in a draw of its own and may fill the uniforms of that draw up to the given room.
    Own(&'r FieldRoom),
    /// The programs run inside the draws of other programs, such as clips stacked in one draw. The uniforms of such a
    /// draw are shared, so the programs have no uniform array.
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
    /// Compiled when a shape first needs it. Contains the program, or the error from compiling it.
    program: OnceCell<Result<Program<U, C>, Rc<Error>>>,
}

impl<U: Uniforms, C: Children> Family<U, C> {
    /// Compiles the shortest program right away. The longer programs compile when a shape first needs them. A family
    /// whose uniforms have no array contains only one program.
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
        // Each program has twice the columns of the one before. A shape then fills at least half of the columns read
        // by its program, and a family compiles only a few programs.
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

    /// The number of columns in the longest program of the family.
    pub fn most(&self) -> usize {
        self.sizes.last().map_or(0, |size| size.columns)
    }

    /// The shortest program with at least `columns` columns, compiled when it is first needed. Fails when no program
    /// is that long or when the program fails to compile.
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
