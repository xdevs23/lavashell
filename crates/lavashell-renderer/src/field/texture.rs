// SPDX-License-Identifier: GPL-3.0-only
use super::Fields;
use super::program::{Part, SMOOTHING, uniform_block};
use crate::Error;
use crate::color::channel_step;
use crate::failure::{DrawFailure, FailedDraw};
use crate::paint::{device_scale, max_image_side, rect_of, skrect};
use crate::scalar::{scalar, scalars};
use lavashell_core::numbers::whole_count;
use lavashell_core::prelude::*;
use skia_safe::{
    AlphaType, BlendMode, Blender, Canvas, ColorType, FilterMode, ISize, Image, ImageInfo, Matrix,
    Paint, SamplingOptions, Shader, Surface, TileMode,
};
use std::ops::Index;
use std::rc::Rc;

/// The number of texels outside a region that linear sampling at the edge of the region blends in. A sample at the
/// edge reads half a texel outward, which lies in the next texel.
const LINEAR_SAMPLING_TEXELS: f64 = 1.0;

/// The texels by which the gradient of a field texture steps from the point it reads (`h` in `texture.sksl`).
const GRADIENT_STEP_TEXELS: f64 = 1.0;

/// The texels added around the region needed from a texture. With them, every read at the edge of the region blends
/// only texels that contain the field, and so does every read of a gradient step.
const BORDER_TEXELS: f64 = LINEAR_SAMPLING_TEXELS + GRADIENT_STEP_TEXELS;

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Content {
    Distance,
    Lens,
}

impl Content {
    fn name(self) -> &'static str {
        match self {
            Self::Distance => "distance texels",
            Self::Lens => "lens texels",
        }
    }

    pub fn texel(self) -> Part {
        Part::new(match self {
            Self::Distance => include_str!("texel/distance.sksl"),
            Self::Lens => include_str!("texel/lens.sksl"),
        })
    }

    pub fn fold(self) -> Part {
        Part::new(match self {
            Self::Distance => include_str!("texel/fold_distance.sksl"),
            Self::Lens => include_str!("texel/fold_lens.sksl"),
        })
    }

    pub fn limit(self) -> Part {
        Part::new(match self {
            Self::Distance => include_str!("texel/limit_distance.sksl"),
            Self::Lens => include_str!("texel/limit_lens.sksl"),
        })
    }
}

uniform_block!(Smoothing { k: 1 });

/// The side of a texel of a field texture in device pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TexelPixels(f64);

impl TexelPixels {
    /// One texel per device pixel, the finest field that a pixel shows.
    pub const DEVICE_PIXEL: Self = Self(1.0);

    /// The texel size for a lens field on `canvas`. At this size, linear interpolation between texels is off by at
    /// most one step of the coarsest color channel of the canvas. Interpolating a field of curvature `k` over texels
    /// of side `h` is off by at most `k h^2 / 8`. A lens field rounds every corner over at least `SMOOTHING`, so `k`
    /// is at most `1 / SMOOTHING`. Returns None for a canvas without pixels or with a transform that collapses every
    /// length.
    pub fn lens(canvas: &Canvas) -> Option<Self> {
        let tolerance = channel_step(canvas.image_info().color_type())?;
        let sharpest_rounding = SMOOTHING * f64::from(device_scale(canvas)?);
        Some(Self((8.0 * tolerance * sharpest_rounding).sqrt()))
    }
}

pub(super) struct ByContent<T> {
    distance: T,
    lens: T,
}

impl<T> ByContent<T> {
    pub fn new<E>(value: impl Fn(Content, &'static str) -> Result<T, E>) -> Result<Self, E> {
        Ok(Self {
            distance: value(Content::Distance, Content::Distance.name())?,
            lens: value(Content::Lens, Content::Lens.name())?,
        })
    }
}

impl<T> Index<Content> for ByContent<T> {
    type Output = T;

    fn index(&self, content: Content) -> &T {
        match content {
            Content::Distance => &self.distance,
            Content::Lens => &self.lens,
        }
    }
}

#[derive(Clone)]
pub(crate) struct FieldTexture {
    image: Image,
    grid: Grid,
}

impl FieldTexture {
    pub(super) fn new(
        canvas: &Canvas,
        fields: &Fields,
        content: Content,
        shape: &Shape,
        grid: Grid,
    ) -> Result<Self, FailedDraw> {
        let mut surface = grid.surface(canvas)?;
        let field = fields.texels(content, &grid, surface.canvas(), shape)?;
        grid.fill(surface.canvas(), field, &replace())?;
        Ok(Self {
            image: surface.image_snapshot(),
            grid,
        })
    }

    pub fn shader(&self) -> Result<Shader, DrawFailure> {
        self.grid.shader(&self.image, FilterMode::Linear)
    }

    pub fn texel(&self) -> f64 {
        self.grid.texel
    }

    pub(super) fn grid(&self) -> Grid {
        self.grid
    }
}

#[derive(Clone, Copy, PartialEq)]
pub(super) struct Grid {
    region: Rect,
    texel: f64,
    size: ISize,
}

impl Grid {
    /// The texels over the visible part of `shape` and `reach` dp around it. The texels have the side `pixels`, or a
    /// coarser side where a texture with texels of that side would exceed the longest image side rendered by the
    /// canvas. Returns None when no part of `shape` is visible or the transform of the canvas has no scale.
    ///
    /// # Errors
    /// Fails when the number of texels along a side of the region does not fit the size of a Skia image.
    pub fn around(
        canvas: &Canvas,
        shape: &Shape,
        reach: f64,
        pixels: TexelPixels,
    ) -> Result<Option<Self>, FailedDraw> {
        let (Some(clip), Some(scale)) = (canvas.local_clip_bounds(), device_scale(canvas)) else {
            return Ok(None);
        };
        let clip = rect_of(clip);
        let region = |texel: f64| {
            let margin = Dp::new(reach.max(0.0) + BORDER_TEXELS * texel);
            shape
                .bounds()
                .outset(margin)
                .intersection(clip.outset(margin))
        };
        let finest = pixels.0 / f64::from(scale);
        let Some(at_finest) = region(finest) else {
            return Ok(None);
        };
        // The longest side spans `inner + 2 BORDER_TEXELS texel` dp, which fits `max_side` texels from this texel on.
        let inner = at_finest.width.max(at_finest.height).get() - 2.0 * BORDER_TEXELS * finest;
        let max_side = f64::from(max_image_side(canvas));
        let texel = finest.max(inner / (max_side - 2.0 * BORDER_TEXELS));
        (region(texel)
            .map(|region| Self::new(region, texel))
            .transpose())
        .map_err(failed_allocation)
    }

    /// The texels of side `texel` dp that cover `region` from its top left corner on.
    fn new(region: Rect, texel: f64) -> Result<Self, Error> {
        let texels = |length: Dp| -> Result<i32, Error> {
            let count = whole_count(texels_along(length, texel)).map_err(Error::TexelCount)?;
            i32::try_from(count).map_err(Error::TexelSide)
        };
        let size = ISize::new(texels(region.width)?, texels(region.height)?);
        let covered = |texels: i32| Dp::new(f64::from(texels) * texel);
        Ok(Self {
            region: Rect::new(
                region.x,
                region.y,
                covered(size.width),
                covered(size.height),
            ),
            texel,
            size,
        })
    }

    pub fn texel(&self) -> f64 {
        self.texel
    }

    pub fn covers(&self, other: &Self) -> bool {
        let (outer, inner) = (self.region, other.region);
        self.texel.to_bits() == other.texel.to_bits()
            && outer.x <= inner.x
            && outer.y <= inner.y
            && inner.x + inner.width <= outer.x + outer.width
            && inner.y + inner.height <= outer.y + outer.height
    }

    /// The texels over the regions of `self` and `other` together, or `other` alone where the joined texels would
    /// exceed the longest image side rendered by `canvas`.
    ///
    /// # Errors
    /// Fails when the number of texels along a side of the joined region does not fit the size of a Skia image.
    pub fn joined(&self, other: &Self, canvas: &Canvas) -> Result<Self, FailedDraw> {
        let region = self.region.union(other.region);
        let max_side = f64::from(max_image_side(canvas));
        let fits = |length: Dp| texels_along(length, self.texel) <= max_side;
        if fits(region.width) && fits(region.height) {
            Self::new(region, self.texel).map_err(failed_allocation)
        } else {
            Ok(*other)
        }
    }

    /// A new surface whose canvas maps dp onto texels. The first draw on the
    /// surface must replace every texel, since the surface starts with undefined texels.
    fn surface(&self, canvas: &Canvas) -> Result<Surface, DrawFailure> {
        let info = ImageInfo::new(self.size, ColorType::RGBAF16, AlphaType::Premul, None);
        let mut surface = canvas
            .new_surface(&info, None)
            .ok_or(DrawFailure::Allocation)?;
        let scale = scalar(1.0 / self.texel)?;
        let origin = scalars([-self.region.x.get(), -self.region.y.get()])?;
        let target = surface.canvas();
        let _chained_canvas = target.scale((scale, scale));
        let _chained_canvas = target.translate((origin[0], origin[1]));
        Ok(surface)
    }

    fn shader(&self, image: &Image, filter: FilterMode) -> Result<Shader, DrawFailure> {
        let [x, y, texel] = scalars([self.region.x.get(), self.region.y.get(), self.texel])?;
        let placement = Matrix::concat(&Matrix::translate((x, y)), &Matrix::scale((texel, texel)));
        image
            .to_raw_shader(
                (TileMode::Clamp, TileMode::Clamp),
                SamplingOptions::from(filter),
                &placement,
            )
            .ok_or(DrawFailure::ShaderRejected)
    }

    fn texels(&self, mut surface: Surface) -> Result<Shader, DrawFailure> {
        self.shader(&surface.image_snapshot(), FilterMode::Nearest)
    }

    fn fill(&self, target: &Canvas, shader: Shader, blender: &Blender) -> Result<(), DrawFailure> {
        let _chained_canvas = target.draw_rect(
            skrect(self.region)?,
            Paint::default()
                .set_shader(shader)
                .set_blender(blender.clone()),
        );
        Ok(())
    }

    /// Draws `fields` into one texture and returns its texels. The first field
    /// replaces every texel. Each later field is combined through `join` with the
    /// texels drawn before it.
    pub fn combined(
        &self,
        target: &Canvas,
        fields: impl IntoIterator<Item = Result<Shader, FailedDraw>>,
        join: &Blender,
    ) -> Result<Shader, FailedDraw> {
        let mut surface = self.surface(target)?;
        let replace = replace();
        for (index, field) in fields.into_iter().enumerate() {
            let blender = if index == 0 { &replace } else { join };
            self.fill(surface.canvas(), field?, blender)?;
        }
        self.texels(surface).map_err(FailedDraw::from)
    }
}

fn failed_allocation(error: Error) -> FailedDraw {
    FailedDraw::caused(DrawFailure::Allocation, Rc::new(error))
}

/// The number of texels of side `texel` dp that cover `length` dp.
fn texels_along(length: Dp, texel: f64) -> f64 {
    (length.get() / texel).ceil()
}

fn replace() -> Blender {
    Blender::mode(BlendMode::Src)
}
