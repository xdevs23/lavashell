// SPDX-License-Identifier: GPL-3.0-only
use super::{Analytic, Data, DataFold, Folds, Kind, KindFamily, Kinds, Route, Texels, hash_bits};
use crate::failure::{DrawFailure, FailedDraw};
use crate::field::family::{Family, Pass};
use crate::field::outline::{ContourPath, OutlinePaths};
use crate::field::program::{
    Layout, Part, Program, Uniforms, children, count_uniform, shader_effect, uniform_block,
};
use crate::field::room::FieldRoom;
use crate::field::sksl::Type;
use crate::field::texture::{Content, Grid};
use crate::field::{Fields, SEGMENT_FLOATS};
use crate::retention::Frames;
use lavashell_core::prelude::*;
use skia_safe::{Canvas, Shader};
use std::collections::hash_map::DefaultHasher;
use std::iter;
use std::rc::Rc;

uniform_block!(Count {
    count: 1,
    lens_radius: 1,
});

const SEGMENT: Part = Part::new(include_str!("contour/segment.sksl"));
const CONTOUR: Part = Part::new(include_str!("contour.sksl"));
const CHUNK: Part = Part::new(include_str!("contour/chunk.sksl"));

/// The segments of a long outline that one pass over the texels reads, one segment in each column. Each pass fills
/// as many uniforms as the device allows. Every pass combines its lens values with those of the earlier passes in
/// half floats, so fewer passes lose less lens precision.
struct Segments(Vec<f64>);

impl Uniforms for Segments {
    const COLUMN_STEP: usize = 1;

    fn layout(columns: usize, names: &mut Layout) {
        names.push(("segments".into(), columns * SEGMENT_FLOATS));
    }

    fn columns(&self) -> usize {
        self.0.len() / SEGMENT_FLOATS
    }

    fn write(&self, columns: usize, floats: &mut Vec<f64>) {
        let padding = (columns * SEGMENT_FLOATS).saturating_sub(self.0.len());
        floats.extend(self.0.iter().copied().chain(iter::repeat_n(0.0, padding)));
    }
}

fn lens_radius(polygon: &Polygon) -> f64 {
    let bounds = polygon.bounds();
    (bounds.width.min(bounds.height) / 2.0).get()
}

fn count(segments: &[f64], lens_radius: f64) -> Result<Count, crate::Error> {
    Ok(Count {
        count: [count_uniform(segments.len() / SEGMENT_FLOATS)?],
        lens_radius: [lens_radius],
    })
}

fn chunk(segments: &[f64], lens_radius: f64) -> Result<(Segments, Count), FailedDraw> {
    let count = count(segments, lens_radius).map_err(outline_length)?;
    Ok((Segments(segments.to_vec()), count))
}

fn outline_length(cause: crate::Error) -> FailedDraw {
    FailedDraw::caused(DrawFailure::OutlineLength, Rc::new(cause))
}

pub(super) struct Contour {
    chunks: Chunks,
    paths: OutlinePaths<Vec<u64>, Rc<ContourPath>>,
}

impl Contour {
    pub fn new(room: &FieldRoom) -> Result<Self, crate::Error> {
        Ok(Self {
            chunks: Chunks::new(room)?,
            paths: OutlinePaths::default(),
        })
    }

    pub fn retain_recent(&mut self, frames: &Frames) {
        self.paths.retain_recent(frames);
    }

    fn path(&self, frames: &Frames, polygon: &Polygon) -> Rc<ContourPath> {
        let source = polygon.points().iter().flat_map(|p| [p.x.get(), p.y.get()]);
        let key = source.map(f64::to_bits).collect();
        self.paths
            .get_or_make(key, frames, || Rc::new(ContourPath::new(polygon)))
    }
}

impl Folds for Contour {
    const FOLDS: &'static [DataFold] = &[
        DataFold {
            function: "foldClosest",
            value: Type::Float4,
            step: "closestStep",
        },
        DataFold {
            function: "foldLensSums",
            value: Type::Float4,
            step: "lensSumsStep",
        },
    ];
}

impl Kind for Contour {
    type Shape = Polygon;
    type Uniforms = (Data<Self>, Count);

    const NAME: &'static str = "contour";
    const PARTS: &'static [Part] = &[SEGMENT, CONTOUR];
    const LENS: Option<Part> = Some(Part::new(include_str!("contour_lens.sksl")));

    /// Fits when the segments of the outline fit into `most` columns, one segment in each column.
    fn fit(
        &self,
        kinds: &Kinds,
        polygon: &Polygon,
        most: usize,
    ) -> Result<(Data<Self>, Count), &dyn Texels<Polygon>> {
        let path = self.path(kinds.frames(), polygon);
        let segments = path.segments();
        let exceeding: &dyn Texels<Polygon> = self;
        let data = Data::new(segments, most).ok_or(exceeding)?;
        // A segment count that an `f32` cannot represent exactly sends the contour to a texture. The texture counts
        // the segments of each chunk separately and needs no count of the whole outline.
        let Ok(count) = count(segments, lens_radius(polygon)) else {
            return Err(exceeding);
        };
        Ok((data, count))
    }

    fn hash(&self, _: &Kinds, polygon: &Polygon, hasher: &mut DefaultHasher) {
        hash_polygon(polygon, hasher);
    }

    fn route(&self, kinds: &Kinds, polygon: &Polygon) -> Result<Route, DrawFailure> {
        self.path(kinds.frames(), polygon)
            .outline
            .clone()
            .map(Route::Path)
    }

    fn family<B, C>(programs: &Analytic<B, C>) -> Option<&KindFamily<Self, B, C>> {
        programs.contour.as_ref()
    }
}

impl Texels<Polygon> for Contour {
    /// The lens texels read the exact field texture that the frame already built
    /// on the same grid. They draw the exact field again only when the frame has
    /// no such texture.
    fn texels(
        &self,
        fields: &Fields,
        content: Content,
        grid: &Grid,
        target: &Canvas,
        polygon: &Polygon,
    ) -> Result<Shader, FailedDraw> {
        let path = self.path(fields.frames(), polygon);
        let segments = path.segments();
        match content {
            Content::Distance => self.chunks.distance(grid, target, segments),
            Content::Lens => {
                let distance = match fields.exact_on(grid, &Shape::Contour(polygon.clone()))? {
                    Some(exact) => exact,
                    None => self.chunks.distance(grid, target, segments)?,
                };
                let lens_radius = lens_radius(polygon);
                self.chunks
                    .lens(grid, target, segments, lens_radius, distance)
            }
        }
    }
}

fn hash_polygon(polygon: &Polygon, hasher: &mut DefaultHasher) {
    hash_bits(
        polygon.points().iter().flat_map(|p| [p.x.get(), p.y.get()]),
        hasher,
    );
}

children!(Nearest { segments });

children!(Exact { field });

children!(Means { means, field });

uniform_block!(Total { chunks: 1 });

struct Chunks {
    distance: Family<(Segments, Count)>,
    signed: Program<(), Nearest>,
    nearest: Program<()>,
    lens: Family<((Segments, Count), Total), Exact>,
    mean: Program<Count, Means>,
    join: Program<()>,
}

impl Chunks {
    fn new(room: &FieldRoom) -> Result<Self, crate::Error> {
        let chunk = |main| [SEGMENT, CHUNK, Part::new(main)];
        let pass = Pass::Own(room);
        Ok(Self {
            distance: Family::new(
                "segment distance",
                &chunk(include_str!("contour/distance.sksl")),
                shader_effect,
                pass,
            )?,
            signed: Program::shader(
                "signed distance",
                &[Part::new(include_str!("contour/signed.sksl"))],
            )?,
            nearest: Program::blender(
                "nearest segment",
                &[Part::new(include_str!("contour/nearest.sksl"))],
            )?,
            lens: Family::new(
                "segment lens",
                &chunk(include_str!("contour/lens.sksl")),
                shader_effect,
                pass,
            )?,
            mean: Program::shader(
                "lens mean",
                &[SEGMENT, Part::new(include_str!("contour/mean.sksl"))],
            )?,
            join: Program::blender("join", &[Part::new(include_str!("contour/join.sksl"))])?,
        })
    }

    fn distance(
        &self,
        grid: &Grid,
        target: &Canvas,
        segments: &[f64],
    ) -> Result<Shader, FailedDraw> {
        let chunks = segments
            .chunks(self.distance.most() * SEGMENT_FLOATS)
            .map(|segments| {
                let uniforms = chunk(segments, 0.0)?;
                let program = self.distance.holding(uniforms.columns())?;
                program.evaluate(&uniforms, ()).map_err(FailedDraw::from)
            });
        let segments = grid.combined(target, chunks, &self.nearest.blend(&())?)?;
        self.signed
            .evaluate(&(), Nearest { segments })
            .map_err(FailedDraw::from)
    }

    /// Each chunk of segments writes the means of its lens sums together with their weight. The `join` blender
    /// combines these means with the means of the earlier chunks by weight. Each weight is divided by the number of
    /// chunks, so means and weights lie within the range of half floats.
    fn lens(
        &self,
        grid: &Grid,
        target: &Canvas,
        segments: &[f64],
        lens_radius: f64,
        distance: Shader,
    ) -> Result<Shader, FailedDraw> {
        let chunk_floats = self.lens.most() * SEGMENT_FLOATS;
        let total = Total {
            chunks: [count_uniform(segments.len().div_ceil(chunk_floats)).map_err(outline_length)?],
        };
        let chunks = segments.chunks(chunk_floats).map(|segments| {
            let field = distance.clone();
            let uniforms = (chunk(segments, lens_radius)?, total);
            let program = self.lens.holding(uniforms.columns())?;
            program
                .evaluate(&uniforms, Exact { field })
                .map_err(FailedDraw::from)
        });
        let means = grid.combined(target, chunks, &self.join.blend(&())?)?;
        let children = Means {
            means,
            field: distance,
        };
        let mean = Count {
            count: [0.0],
            lens_radius: [lens_radius],
        };
        self.mean
            .evaluate(&mean, children)
            .map_err(FailedDraw::from)
    }
}
