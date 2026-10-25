// SPDX-License-Identifier: GPL-3.0-only
//! Drawing and clipping paths through tiles. A path larger than one tile is cut into tiles on a grid that moves with
//! the path.

use super::geometry::pixel_bytes;
use super::key::TileContent;
use super::tiles::{self, TileSpec};
use super::{EDGE_PIXELS, RasterCache, resource_cache_limit};
use crate::failure::DrawFailure;
use crate::paint::{device_scale, max_image_side};
use skia_safe::{
    BlendMode, Canvas, ClipOp, Data, Flattenable, IRect, Matrix, Paint, Path, Rect, RoundOut,
    Shader, Size, TileMode, canvas::SaveLayerRec, paint::Style,
};
use std::hash::{Hash, Hasher};
use std::rc::Rc;

impl RasterCache {
    pub fn draw(&mut self, canvas: &Canvas, path: &Path, paint: &Paint) -> Result<(), DrawFailure> {
        let tiles = match self.place(canvas, path, paint) {
            Placement::Tiles(tiles) => tiles,
            Placement::Hidden => return Ok(()),
            Placement::Vector => {
                let _chained_canvas = canvas.draw_path(path, paint);
                return Ok(());
            }
        };
        // The tiles contain the path without its image filter. A layer applies
        // the filter to all tiles together.
        let mut shape = paint.clone();
        let _chained_paint = shape.set_image_filter(None);
        let saved = match paint.image_filter() {
            Some(filter) => {
                let mut layer = Paint::default();
                let _chained_paint = layer.set_image_filter(filter.clone());
                let bounds = filter.compute_fast_bounds(tiles.covered());
                canvas.save_layer(&SaveLayerRec::default().bounds(&bounds).paint(&layer))
            }
            None => canvas.save(),
        };
        let drawn = tiles.iter().try_for_each(|tile| {
            self.draw_tile(canvas, &tile.spec, |c| tile.rasterize(c, path, &shape))
        });
        let _chained_canvas = canvas.restore_to_count(saved);
        drawn
    }

    /// Clips `canvas` by the coverage of the pixels in the tiles of `path`.
    pub fn clip(&mut self, canvas: &Canvas, path: &Path, op: ClipOp) -> Result<(), DrawFailure> {
        let mut coverage = Paint::default();
        let _chained_paint = coverage.set_anti_alias(true);
        let tiles = match self.place(canvas, path, &coverage) {
            Placement::Tiles(tiles) => tiles,
            Placement::Hidden => {
                if op == ClipOp::Intersect {
                    let _chained_canvas = canvas.clip_rect(Rect::new_empty(), None, false);
                }
                return Ok(());
            }
            Placement::Vector => {
                let _chained_canvas = canvas.clip_path(path, op, true);
                return Ok(());
            }
        };
        let mut shader = self.tile_shader(canvas, &tiles.first, path, &coverage)?;
        for tile in &tiles.rest {
            let covered = self.tile_shader(canvas, tile, path, &coverage)?;
            shader = skia_safe::shaders::blend(BlendMode::Plus, shader, covered);
        }
        let _chained_canvas = canvas.clip_shader(shader, op);
        Ok(())
    }

    fn tile_shader(
        &mut self,
        canvas: &Canvas,
        tile: &PathTile,
        path: &Path,
        paint: &Paint,
    ) -> Result<Shader, DrawFailure> {
        let image = self.tile_image(canvas, &tile.spec, |c| tile.rasterize(c, path, paint))?;
        let destination = tile.spec.destination;
        let mut placement = Matrix::translate((destination.left, destination.top));
        let _chained_matrix = placement.pre_scale((1.0 / tile.scale, 1.0 / tile.scale), None);
        image
            .to_shader(
                (TileMode::Decal, TileMode::Decal),
                tiles::sampling(),
                &placement,
            )
            .ok_or(DrawFailure::ShaderRejected)
    }

    pub(super) fn place(&self, canvas: &Canvas, path: &Path, paint: &Paint) -> Placement {
        if !self.raster {
            return Placement::Vector;
        }
        let Some(region) = Region::new(canvas, path, paint) else {
            return Placement::Hidden;
        };
        let drawing = Rc::new(PathDrawing::new(path, paint, &region));
        let bytes_per_pixel = canvas.image_info().bytes_per_pixel();
        match region.tiles(&drawing, bytes_per_pixel) {
            Some(tiles) => Placement::Tiles(tiles),
            None => Placement::Hidden,
        }
    }
}

pub(super) enum Placement {
    Tiles(Tiles),
    Hidden,
    /// Skia draws the path directly. Only devices whose path masks work use this.
    Vector,
}

pub(super) struct Tiles {
    first: PathTile,
    rest: Vec<PathTile>,
}

impl Tiles {
    pub fn iter(&self) -> impl Iterator<Item = &PathTile> {
        std::iter::once(&self.first).chain(&self.rest)
    }

    fn covered(&self) -> Rect {
        let destinations = self.rest.iter().map(|tile| tile.spec.destination);
        destinations.fold(self.first.spec.destination, Rect::join2)
    }
}

struct Region {
    bounds: Rect,
    needed: Rect,
    scale: f32,
    /// Device pixels along each side of a grid tile: the longer side of the device clip, so that the clip needs at
    /// most two tiles in each direction and a scroll rasterizes at most one row or column of tiles. A tile is also
    /// no longer than the longest image side rendered by the canvas and no larger than the resource cache limit.
    side: i32,
}

impl Region {
    /// Returns None when no part of the drawn path is inside the clip.
    fn new(canvas: &Canvas, path: &Path, paint: &Paint) -> Option<Self> {
        if path.is_empty() {
            return None;
        }
        let scale = device_scale(canvas)?;
        let half_stroke = paint.stroke_width() / 2.0;
        let margin = half_stroke + EDGE_PIXELS / scale;
        let mut bounds = path.bounds().with_outset((margin, margin));
        let drawn = paint
            .image_filter()
            .map_or(bounds, |filter| filter.compute_fast_bounds(bounds));
        let clip = canvas.local_clip_bounds()?;
        if !Rect::intersects2(drawn, clip) {
            return None;
        }
        let device_clip = canvas.device_clip_bounds()?;
        let cached_side =
            (resource_cache_limit(canvas) / canvas.image_info().bytes_per_pixel()).isqrt();
        let side = (device_clip.width().max(device_clip.height()))
            .min(max_image_side(canvas))
            .min(i32::try_from(cached_side).unwrap_or(i32::MAX));
        if side <= 0 {
            return None;
        }
        let reach = [
            bounds.left - drawn.left,
            bounds.top - drawn.top,
            drawn.right - bounds.right,
            drawn.bottom - bounds.bottom,
        ]
        .into_iter()
        .fold(0.0, f32::max);
        let needed = clip.with_outset((reach, reach));
        let matrix = canvas.local_to_device_as_3x3();
        let translate = (matrix.translate_x(), matrix.translate_y());
        if matrix == Matrix::scale_translate((scale, scale), translate) {
            let shift =
                |local: f32, translate: f32| (local * scale + translate).rem_euclid(1.0) / scale;
            bounds.left -= shift(bounds.left, translate.0);
            bounds.top -= shift(bounds.top, translate.1);
        }
        Some(Self {
            bounds,
            needed,
            scale,
            side,
        })
    }

    /// One tile for a path that fits into one tile. Otherwise the tiles of the grid that cover the part of the
    /// region needed by the clip. Returns None when the region has no pixels.
    fn tiles(&self, drawing: &Rc<PathDrawing>, bytes_per_pixel: usize) -> Option<Tiles> {
        let size = Size::new(self.bounds.width(), self.bounds.height()) * self.scale;
        let pixels = IRect::from_size(size.to_ceil());
        if pixels.is_empty() {
            return None;
        }
        let mut tiles = if pixels.width() <= self.side && pixels.height() <= self.side {
            vec![self.tile(drawing, pixels, bytes_per_pixel)?]
        } else {
            let needed = Rect::new(
                (self.needed.left - self.bounds.left) * self.scale,
                (self.needed.top - self.bounds.top) * self.scale,
                (self.needed.right - self.bounds.left) * self.scale,
                (self.needed.bottom - self.bounds.top) * self.scale,
            );
            let needed = IRect::intersect(&needed.round_out(), &pixels)?;
            let columns = needed.left / self.side..=(needed.right - 1) / self.side;
            let rows = needed.top / self.side..=(needed.bottom - 1) / self.side;
            rows.flat_map(|row| columns.clone().map(move |column| (column, row)))
                .map(|(column, row)| {
                    let cell =
                        IRect::from_xywh(column * self.side, row * self.side, self.side, self.side);
                    let tile = IRect::intersect(&cell, &pixels)?;
                    self.tile(drawing, tile, bytes_per_pixel)
                })
                .collect::<Option<Vec<_>>>()?
        };
        let rest = tiles.split_off(1);
        let first = tiles.pop()?;
        Some(Tiles { first, rest })
    }

    /// The tile for the device `pixels` of the region. Returns None when the byte count of the tile overflows.
    fn tile(
        &self,
        drawing: &Rc<PathDrawing>,
        pixels: IRect,
        bytes_per_pixel: usize,
    ) -> Option<PathTile> {
        let device = Rect::from_irect(pixels);
        let origin = (
            self.bounds.left + device.left / self.scale,
            self.bounds.top + device.top / self.scale,
        );
        let size = pixels.size();
        Some(PathTile {
            spec: TileSpec {
                content: TileContent::Path {
                    drawing: drawing.clone(),
                    start: (pixels.left, pixels.top),
                },
                bytes: pixel_bytes(size, bytes_per_pixel)?,
                size,
                destination: Rect::from_xywh(
                    origin.0,
                    origin.1,
                    device.width() / self.scale,
                    device.height() / self.scale,
                ),
            },
            origin,
            scale: self.scale,
        })
    }
}

/// What the tiles of a path contain: the path, how it is painted, and its bounds and scale on the device.
pub(crate) struct PathDrawing {
    path: Path,
    shader: Option<Data>,
    /// The bits of the color, the stroke width, the bounds and the scale.
    numbers: [u32; 10],
    style: Style,
}

impl PathDrawing {
    fn new(path: &Path, paint: &Paint, region: &Region) -> Self {
        let color = paint.color4f();
        let bounds = region.bounds;
        let numbers = [
            color.a,
            color.r,
            color.g,
            color.b,
            paint.stroke_width(),
            bounds.left,
            bounds.top,
            bounds.right,
            bounds.bottom,
            region.scale,
        ];
        Self {
            path: path.clone(),
            shader: paint.shader().map(|shader| shader.serialize()),
            numbers: numbers.map(f32::to_bits),
            style: paint.style(),
        }
    }
}

impl PartialEq for PathDrawing {
    fn eq(&self, other: &Self) -> bool {
        self.numbers == other.numbers
            && self.style == other.style
            && self.shader == other.shader
            && self.path == other.path
    }
}

impl Eq for PathDrawing {}

/// Equal paths have equal point counts, and equal shaders equal bytes.
impl Hash for PathDrawing {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.numbers.hash(state);
        self.style.hash(state);
        self.path.count_points().hash(state);
        self.shader.as_ref().map(Data::as_bytes).hash(state);
    }
}

pub(super) struct PathTile {
    pub spec: TileSpec,
    origin: (f32, f32),
    scale: f32,
}

impl PathTile {
    fn rasterize(&self, tile: &Canvas, path: &Path, paint: &Paint) {
        let _chained_canvas = tile.scale((self.scale, self.scale));
        let _chained_canvas = tile.translate((-self.origin.0, -self.origin.1));
        let _chained_canvas = tile.draw_path(path, paint);
    }
}
