// SPDX-License-Identifier: GPL-3.0-only

use super::geometry::{pixel_aligned_translate, pixel_bytes, pixel_phase};
use super::key::{ExactF32, TileContent};
use super::tiles::{self, TileSpec};
use super::{EDGE_PIXELS, RasterCache, RasterMode, resource_cache_limit};
use crate::failure::DrawFailure;
use crate::paint::{device_scale, max_image_side};
use skia_safe::{
    BlendMode, Canvas, ClipOp, Data, Flattenable, IRect, Matrix, Paint, Path, Point, Rect,
    RoundOut, Shader, Size, TileMode, canvas::SaveLayerRec, paint::Style,
};
use std::hash::{Hash, Hasher};
use std::rc::Rc;

impl RasterCache {
    pub fn draw(&mut self, canvas: &Canvas, path: &Path, paint: &Paint) -> Result<(), DrawFailure> {
        let tiles = match self.place(canvas, path, paint)? {
            Placement::Tiles(tiles) => tiles,
            Placement::Hidden => return Ok(()),
            Placement::Vector => {
                let _canvas_returned_for_chaining = canvas.draw_path(path, paint);
                return Ok(());
            }
        };
        let mut shape = paint.clone();
        let _paint_returned_for_chaining = shape.set_image_filter(None);
        let saved = match paint.image_filter() {
            Some(filter) => {
                let mut layer = Paint::default();
                let _paint_returned_for_chaining = layer.set_image_filter(filter.clone());
                let bounds = filter.compute_fast_bounds(tiles.covered());
                canvas.save_layer(&SaveLayerRec::default().bounds(&bounds).paint(&layer))
            }
            None => canvas.save(),
        };
        let drawn = tiles.iter().try_for_each(|tile| {
            self.draw_tile(canvas, &tile.spec, |c| tile.rasterize(c, path, &shape))
        });
        let _canvas_returned_for_chaining = canvas.restore_to_count(saved);
        drawn
    }

    pub fn clip(&mut self, canvas: &Canvas, path: &Path, op: ClipOp) -> Result<(), DrawFailure> {
        let mut coverage = Paint::default();
        let _paint_returned_for_chaining = coverage.set_anti_alias(true);
        let tiles = match self.place(canvas, path, &coverage)? {
            Placement::Tiles(tiles) => tiles,
            Placement::Hidden => {
                if op == ClipOp::Intersect {
                    let _canvas_returned_for_chaining =
                        canvas.clip_rect(Rect::new_empty(), None, false);
                }
                return Ok(());
            }
            Placement::Vector => {
                let _canvas_returned_for_chaining = canvas.clip_path(path, op, true);
                return Ok(());
            }
        };
        let mut shader = self.tile_shader(canvas, &tiles.first, path, &coverage)?;
        for tile in &tiles.rest {
            let covered = self.tile_shader(canvas, tile, path, &coverage)?;
            shader = skia_safe::shaders::blend(BlendMode::Plus, shader, covered);
        }
        let _canvas_returned_for_chaining = canvas.clip_shader(shader, op);
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
        let _matrix_returned_for_chaining =
            placement.pre_scale((1.0 / tile.scale, 1.0 / tile.scale), None);
        image
            .to_shader(
                (TileMode::Decal, TileMode::Decal),
                tiles::sampling(),
                &placement,
            )
            .ok_or(DrawFailure::ShaderRejected)
    }

    pub(super) fn place(
        &self,
        canvas: &Canvas,
        path: &Path,
        paint: &Paint,
    ) -> Result<Placement, DrawFailure> {
        if self.mode == RasterMode::Vector {
            return Ok(Placement::Vector);
        }
        let Some(region) = Region::new(canvas, path, paint) else {
            return Ok(Placement::Hidden);
        };
        let drawing = Rc::new(PathDrawing::new(path, paint, &region));
        let bytes_per_pixel = canvas.image_info().bytes_per_pixel();
        let tiles = region.tiles(&drawing, bytes_per_pixel)?;
        Ok(tiles.map_or(Placement::Hidden, Placement::Tiles))
    }
}

pub(super) enum Placement {
    Tiles(Tiles),
    Hidden,
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
    side: i32,
}

impl Region {
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
        if let Some(translate) = pixel_aligned_translate(canvas, scale) {
            let phase = pixel_phase(bounds.tl(), scale, translate);
            bounds.left -= phase.x;
            bounds.top -= phase.y;
        }
        Some(Self {
            bounds,
            needed,
            scale,
            side,
        })
    }

    fn tiles(
        &self,
        drawing: &Rc<PathDrawing>,
        bytes_per_pixel: usize,
    ) -> Result<Option<Tiles>, DrawFailure> {
        let size = Size::new(self.bounds.width(), self.bounds.height()) * self.scale;
        let pixels = IRect::from_size(size.to_ceil());
        if pixels.is_empty() {
            return Ok(None);
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
            let Some(needed) = IRect::intersect(&needed.round_out(), &pixels) else {
                return Ok(None);
            };
            let columns = needed.left / self.side..=(needed.right - 1) / self.side;
            let rows = needed.top / self.side..=(needed.bottom - 1) / self.side;
            rows.flat_map(|row| columns.clone().map(move |column| (column, row)))
                .filter_map(|(column, row)| {
                    let cell =
                        IRect::from_xywh(column * self.side, row * self.side, self.side, self.side);
                    IRect::intersect(&cell, &pixels)
                })
                .map(|tile| self.tile(drawing, tile, bytes_per_pixel))
                .collect::<Result<Vec<_>, _>>()?
        };
        let rest = tiles.split_off(1);
        Ok(tiles.pop().map(|first| Tiles { first, rest }))
    }

    fn tile(
        &self,
        drawing: &Rc<PathDrawing>,
        pixels: IRect,
        bytes_per_pixel: usize,
    ) -> Result<PathTile, DrawFailure> {
        let device = Rect::from_irect(pixels);
        let origin = Point::new(
            self.bounds.left + device.left / self.scale,
            self.bounds.top + device.top / self.scale,
        );
        let size = pixels.size();
        Ok(PathTile {
            spec: TileSpec {
                retained_as: Some(TileContent::Path {
                    drawing: drawing.clone(),
                    start: (pixels.left, pixels.top),
                }),
                bytes: pixel_bytes(size, bytes_per_pixel).ok_or(DrawFailure::TileSize)?,
                size,
                destination: Rect::from_xywh(
                    origin.x,
                    origin.y,
                    device.width() / self.scale,
                    device.height() / self.scale,
                ),
            },
            origin,
            scale: self.scale,
        })
    }
}

#[derive(PartialEq)]
pub(crate) struct PathDrawing {
    color: [ExactF32; 4],
    stroke_width: ExactF32,
    bounds: [ExactF32; 4],
    scale: ExactF32,
    style: Style,
    shader: Option<Data>,
    path: Path,
}

impl PathDrawing {
    fn new(path: &Path, paint: &Paint, region: &Region) -> Self {
        let color = paint.color4f();
        let bounds = region.bounds;
        Self {
            color: [color.a, color.r, color.g, color.b].map(ExactF32::from),
            stroke_width: paint.stroke_width().into(),
            bounds: [bounds.left, bounds.top, bounds.right, bounds.bottom].map(ExactF32::from),
            scale: region.scale.into(),
            style: paint.style(),
            shader: paint.shader().map(|shader| shader.serialize()),
            path: path.clone(),
        }
    }
}

impl Eq for PathDrawing {}

impl Hash for PathDrawing {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.color.hash(state);
        self.stroke_width.hash(state);
        self.bounds.hash(state);
        self.scale.hash(state);
        self.style.hash(state);
        self.path.count_points().hash(state);
        self.shader.as_ref().map(Data::as_bytes).hash(state);
    }
}

pub(super) struct PathTile {
    pub spec: TileSpec,
    origin: Point,
    scale: f32,
}

impl PathTile {
    fn rasterize(&self, tile: &Canvas, path: &Path, paint: &Paint) {
        let _canvas_returned_for_chaining = tile.scale((self.scale, self.scale));
        let _canvas_returned_for_chaining = tile.translate(-self.origin);
        let _canvas_returned_for_chaining = tile.draw_path(path, paint);
    }
}
