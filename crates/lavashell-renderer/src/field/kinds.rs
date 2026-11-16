// SPDX-License-Identifier: GPL-3.0-only

mod contour;
mod hollow;
mod rounded;
mod union;

use contour::Contour;
use hollow::Hollow;
use rounded::Rounded;
use union::Union;

use super::Fields;
use super::family::{Family, Pass};
use super::outline::OutlinePath;
use super::program::{Bound, Children, Layout, Part, Uniforms, shader_effect};
use super::room::{COLUMN_FLOATS, FieldRoom};
use super::sksl::{Declaration, FoldedMatrix, Type};
use super::texture::{Content, Grid};
use crate::Error;
use crate::failure::{DrawFailure, FailedDraw};
use crate::retention::Frames;
use lavashell_core::prelude::*;
use lavashell_core::surface::SurfaceId;
use skia_safe::{Canvas, Shader};
use std::collections::hash_map::DefaultHasher;
use std::iter;
use std::marker::PhantomData;
use std::rc::Rc;

const ROUNDED: Part = Part::new(include_str!("rounded.sksl"));

const MATRIX_COLUMNS: usize = 4;

pub(super) struct DataFold {
    pub function: &'static str,
    pub value: Type,
    pub step: &'static str,
}

pub(super) trait Folds {
    const FOLDS: &'static [DataFold];
}

pub(super) struct Data<F>(Vec<f64>, PhantomData<fn() -> F>);

fn data_matrix(index: usize) -> String {
    format!("data{index}")
}

impl<F: Folds> Uniforms for Data<F> {
    const COLUMN_STEP: usize = MATRIX_COLUMNS;

    fn layout(columns: usize, names: &mut Layout) {
        let matrix_floats = MATRIX_COLUMNS * COLUMN_FLOATS;
        names.extend((0..columns / MATRIX_COLUMNS).map(|m| (data_matrix(m).into(), matrix_floats)));
    }

    fn declare(columns: usize) -> Vec<Declaration> {
        let matrices = 0..columns / MATRIX_COLUMNS;
        let uniforms = matrices.clone().map(|m| Declaration::UniformMatrix {
            name: data_matrix(m),
        });
        let folded = matrices
            .map(|m| FoldedMatrix {
                name: data_matrix(m),
                first: m * MATRIX_COLUMNS,
            })
            .collect::<Vec<_>>();
        let folds = F::FOLDS.iter().map(|fold| Declaration::Fold {
            function: fold.function,
            value: fold.value,
            step: fold.step,
            matrices: folded.clone(),
        });
        uniforms.chain(folds).collect()
    }

    fn columns(&self) -> usize {
        self.0.len().div_ceil(COLUMN_FLOATS)
    }

    fn write(&self, columns: usize, floats: &mut Vec<f64>) {
        let padding = (columns * COLUMN_FLOATS).saturating_sub(self.0.len());
        floats.extend(self.0.iter().copied().chain(iter::repeat_n(0.0, padding)));
    }
}

impl<F> Data<F> {
    fn new(values: &[f64], most: usize) -> Option<Self> {
        (values.len().div_ceil(COLUMN_FLOATS) <= most).then(|| Self(values.to_vec(), PhantomData))
    }
}

pub(super) trait Kind {
    type Shape;

    type Uniforms: Uniforms + 'static;

    const NAME: &'static str;

    const PARTS: &'static [Part];

    const LENS: Option<Part>;

    fn fit(
        &self,
        kinds: &Kinds,
        shape: &Self::Shape,
        most: usize,
    ) -> Result<Self::Uniforms, &dyn Texels<Self::Shape>>;

    fn hash(&self, kinds: &Kinds, shape: &Self::Shape, hasher: &mut DefaultHasher);

    fn route(&self, kinds: &Kinds, shape: &Self::Shape) -> Result<Route, DrawFailure>;

    fn family<B, C>(programs: &Analytic<B, C>) -> Option<&KindFamily<Self, B, C>>;
}

pub(super) trait Texels<S> {
    fn texels(
        &self,
        fields: &Fields,
        content: Content,
        grid: &Grid,
        target: &Canvas,
        shape: &S,
    ) -> Result<Shader, FailedDraw>;
}

pub(crate) enum Route {
    Rounded(RoundedRect),
    Path(Rc<OutlinePath>),
    Field,
}

pub(super) trait Visit {
    type Output;

    fn visit<K: Kind>(self, kinds: &Kinds, kind: &K, shape: &K::Shape) -> Self::Output;
}

pub(super) trait Visited {
    fn visit_kind<V: Visit>(&self, kinds: &Kinds, visit: V) -> V::Output;
}

impl Visited for Shape {
    fn visit_kind<V: Visit>(&self, kinds: &Kinds, visit: V) -> V::Output {
        match self {
            Self::Rounded(shape) => visit.visit(kinds, &Rounded, shape),
            Self::Union(shape) => visit.visit(kinds, &Union, shape),
            Self::Contour(shape) => visit.visit(kinds, &kinds.contour, shape),
            Self::Hollow(shape) => visit.visit(kinds, &kinds.hollow, shape),
        }
    }
}

impl Visited for UnionMember {
    fn visit_kind<V: Visit>(&self, kinds: &Kinds, visit: V) -> V::Output {
        match self {
            Self::Rounded(shape) => visit.visit(kinds, &Rounded, shape),
            Self::Contour(shape) => visit.visit(kinds, &kinds.contour, shape),
            Self::Hollow(shape) => visit.visit(kinds, &kinds.hollow, shape),
        }
    }
}

pub(super) struct Kinds {
    frames: Frames,
    contour: Contour,
    hollow: Hollow,
}

impl Kinds {
    pub fn new(room: &FieldRoom) -> Result<Self, Error> {
        Ok(Self {
            frames: Frames::default(),
            contour: Contour::new(room)?,
            hollow: Hollow::default(),
        })
    }

    pub fn begin_frame(&mut self, surface: SurfaceId) {
        self.frames.begin(surface);
        self.contour.retain_recent(&self.frames);
        self.hollow.retain_recent(&self.frames);
    }

    pub fn forget_surface(&mut self, surface: SurfaceId) {
        self.frames.forget(surface);
    }

    pub fn frames(&self) -> &Frames {
        &self.frames
    }

    pub fn visit<V: Visit>(&self, shape: &impl Visited, visit: V) -> V::Output {
        shape.visit_kind(self, visit)
    }

    pub fn route(&self, shape: &Shape) -> Result<Route, DrawFailure> {
        struct Routed;
        impl Visit for Routed {
            type Output = Result<Route, DrawFailure>;
            fn visit<K: Kind>(self, kinds: &Kinds, kind: &K, shape: &K::Shape) -> Self::Output {
                kind.route(kinds, shape)
            }
        }
        self.visit(shape, Routed)
    }

    pub fn hash(&self, shape: &impl Visited, hasher: &mut DefaultHasher) {
        struct Hashed<'h>(&'h mut DefaultHasher);
        impl Visit for Hashed<'_> {
            type Output = ();
            fn visit<K: Kind>(self, kinds: &Kinds, kind: &K, shape: &K::Shape) {
                kind.hash(kinds, shape, self.0);
            }
        }
        self.visit(shape, Hashed(hasher));
    }
}

pub(super) type KindFamily<K, B, C> = Family<(<K as Kind>::Uniforms, B), C>;

pub(crate) struct Analytic<B, C = ()> {
    rounded: Option<KindFamily<Rounded, B, C>>,
    union: Option<KindFamily<Union, B, C>>,
    contour: Option<KindFamily<Contour, B, C>>,
    hollow: Option<KindFamily<Hollow, B, C>>,
}

impl<B: Uniforms, C: Children> Analytic<B, C> {
    pub fn new(consumer: &str, reads: Content, parts: &[Part], pass: Pass) -> Result<Self, Error> {
        fn family<K: Kind, B: Uniforms, C: Children>(
            consumer: &str,
            reads: Content,
            parts: &[Part],
            pass: Pass,
        ) -> Result<Option<KindFamily<K, B, C>>, Error> {
            if matches!(pass, Pass::Shared) && <(K::Uniforms, B)>::COLUMN_STEP > 0 {
                return Ok(None);
            }
            let mut all = K::PARTS.to_vec();
            if reads == Content::Lens {
                let Some(lens) = K::LENS else {
                    return Ok(None);
                };
                all.push(lens);
            }
            all.extend_from_slice(parts);
            let name = format!("{consumer} on a {}", K::NAME);
            Family::new(&name, &all, shader_effect, pass).map(Some)
        }
        Ok(Self {
            rounded: family::<Rounded, _, _>(consumer, reads, parts, pass)?,
            union: family::<Union, _, _>(consumer, reads, parts, pass)?,
            contour: family::<Contour, _, _>(consumer, reads, parts, pass)?,
            hollow: family::<Hollow, _, _>(consumer, reads, parts, pass)?,
        })
    }

    pub(super) fn family_of<K: Kind>(&self) -> Option<&KindFamily<K, B, C>> {
        K::family(self)
    }

    pub fn of(
        &self,
        fields: &Fields,
        shape: &Shape,
    ) -> Result<Option<Bound<'_, B, C>>, FailedDraw> {
        struct Of<'a, B, C>(&'a Analytic<B, C>);
        impl<'a, B: Uniforms, C: Children> Visit for Of<'a, B, C> {
            type Output = Result<Option<Bound<'a, B, C>>, FailedDraw>;
            fn visit<K: Kind>(self, kinds: &Kinds, kind: &K, shape: &K::Shape) -> Self::Output {
                let Some(family) = self.0.family_of::<K>() else {
                    return Ok(None);
                };
                let Ok(uniforms) = kind.fit(kinds, shape, family.most()) else {
                    return Ok(None);
                };
                Ok(Some(family.holding(uniforms.columns())?.bind(&uniforms)))
            }
        }
        fields.kinds.visit(shape, Of(self))
    }
}

fn rect(rect: Rect) -> [f64; 4] {
    [rect.x, rect.y, rect.width, rect.height].map(Dp::get)
}

fn hash_bits(values: impl IntoIterator<Item = f64>, hasher: &mut DefaultHasher) {
    use std::hash::Hash;
    for value in values {
        value.to_bits().hash(hasher);
    }
}
