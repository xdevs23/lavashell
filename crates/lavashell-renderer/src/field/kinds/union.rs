// SPDX-License-Identifier: GPL-3.0-only
use super::rounded::Single;
use super::{
    Analytic, Data, DataFold, Folds, Kind, KindFamily, Kinds, ROUNDED, Route, Texels, hash_bits,
    rect,
};
use crate::failure::{DrawFailure, FailedDraw};
use crate::field::program::{Part, count_uniform, uniform_block};
use crate::field::sksl::Type;
use crate::field::texture::{Content, Grid};
use crate::field::{Fields, smoothing};
use lavashell_core::prelude::*;
use lavashell_core::shape;
use skia_safe::{Canvas, Shader};
use std::collections::hash_map::DefaultHasher;

uniform_block!(Fold {
    members: 1,
    blend_width: 1,
    extent: 4,
});

pub(super) struct Union;

impl Folds for Union {
    const FOLDS: &'static [DataFold] = &[
        DataFold {
            function: "foldExact",
            value: Type::Float3,
            step: "exactStep",
        },
        DataFold {
            function: "foldLens",
            value: Type::Float4,
            step: "lensStep",
        },
    ];
}

impl Kind for Union {
    type Shape = shape::Union;
    type Uniforms = (Data<Self>, Fold);

    const NAME: &'static str = "union";
    const PARTS: &'static [Part] = &[ROUNDED, Part::new(include_str!("union.sksl"))];
    const LENS: Option<Part> = Some(Part::new(include_str!("union_lens.sksl")));

    /// Fits when every member is a rounded box and the bounds and radii of all
    /// members fit into `most` columns.
    fn fit(
        &self,
        _: &Kinds,
        union: &shape::Union,
        most: usize,
    ) -> Result<(Data<Self>, Fold), &dyn Texels<shape::Union>> {
        let exceeding: &dyn Texels<shape::Union> = self;
        let members = union
            .members()
            .iter()
            .map(|member| match member {
                Shape::Rounded(rounded) => Some(Single::new(rounded)),
                _ => None,
            })
            .collect::<Option<Vec<_>>>()
            .ok_or(exceeding)?;
        let values = members
            .iter()
            .flat_map(|member| member.bounds.into_iter().chain(member.radii))
            .collect::<Vec<_>>();
        let data = Data::new(&values, most).ok_or(exceeding)?;
        // A count that `f32` cannot represent exactly sends the union to a texture, which folds the members one by
        // one and needs no count.
        let Ok(count) = count_uniform(members.len()) else {
            return Err(exceeding);
        };
        let fold = Fold {
            members: [count],
            blend_width: [smoothing(union)],
            extent: rect(union.bounds()),
        };
        Ok((data, fold))
    }

    fn hash(&self, kinds: &Kinds, union: &shape::Union, hasher: &mut DefaultHasher) {
        hash_bits([smoothing(union)], hasher);
        for member in union.members() {
            kinds.hash(member, hasher);
        }
    }

    /// The outline of a union is where its field is zero. Skia has no path or
    /// shape for that outline.
    fn route(&self, _: &Kinds, _: &shape::Union) -> Result<Route, DrawFailure> {
        Ok(Route::Field)
    }

    fn family<B, C>(programs: &Analytic<B, C>) -> Option<&KindFamily<Self, B, C>> {
        programs.union.as_ref()
    }
}

impl Texels<shape::Union> for Union {
    /// Folds the fields of the members by the smooth minimum, and limits the
    /// result by the field of the bounds of all members.
    fn texels(
        &self,
        fields: &Fields,
        content: Content,
        grid: &Grid,
        target: &Canvas,
        union: &shape::Union,
    ) -> Result<Shader, FailedDraw> {
        let programs = &fields.programs;
        let members = union
            .members()
            .iter()
            .map(|member| fields.texels(content, grid, target, member));
        let fold = programs.fold(content, smoothing(union))?;
        let folded = grid.combined(target, members, &fold)?;
        let limit = fields.texels(content, grid, target, &union.bounds().into());
        grid.combined(target, [Ok(folded), limit], &programs.limit(content)?)
    }
}
