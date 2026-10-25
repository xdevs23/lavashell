// SPDX-License-Identifier: GPL-3.0-only
use super::family::Pass;
use super::kinds::Analytic;
use super::program::{Part, Program, children, uniform_block};
use super::room::FieldRoom;
use super::texture::{ByContent, Content, Smoothing};
use crate::failure::DrawFailure;
use skia_safe::Blender;

uniform_block!(Measure {
    inner: 1,
    outer: 1,
    inner_open: 1,
    outer_open: 1,
    sigma: 1,
    pixel: 1,
    offset: 2,
    saturation: 1,
});

uniform_block!(Texel { texel: 1 });

children!(FieldImage { field });

pub(super) const COVERAGE: Part = Part::new(include_str!("coverage.sksl"));
const TEXTURE: Part = Part::new(include_str!("texture.sksl"));

pub(super) struct Programs {
    pub texels: ByContent<Analytic<()>>,
    fold: ByContent<Program<Smoothing>>,
    limit: ByContent<Program<()>>,
    pub coverage: Analytic<Measure>,
    pub texture_coverage: Program<(Texel, Measure), FieldImage>,
}

impl Programs {
    /// Sizes the texel programs for the uniforms left next to the fold blender. The texels of a union member are
    /// drawn in one draw together with that blender.
    pub fn new(room: &FieldRoom) -> Result<Self, crate::Error> {
        let fold =
            ByContent::new(|content, name| Program::<Smoothing>::blender(name, &[content.fold()]))?;
        Ok(Self {
            texels: ByContent::new(|content, name| {
                let room = (room.beside(fold[content].effect())).map_err(|source| {
                    crate::Error::UniformCount {
                        program: name.to_owned(),
                        source,
                    }
                })?;
                Analytic::new(name, content, &[content.texel()], Pass::Own(&room))
            })?,
            fold,
            limit: ByContent::new(|content, name| Program::blender(name, &[content.limit()]))?,
            coverage: Analytic::new("coverage", Content::Distance, &[COVERAGE], Pass::Shared)?,
            texture_coverage: Program::shader("texture coverage", &[TEXTURE, COVERAGE])?,
        })
    }

    pub fn fold(&self, content: Content, k: f64) -> Result<Blender, DrawFailure> {
        self.fold[content].blend(&Smoothing { k: [k] })
    }

    pub fn limit(&self, content: Content) -> Result<Blender, DrawFailure> {
        self.limit[content].blend(&())
    }
}
