// SPDX-License-Identifier: GPL-3.0-only
use super::Coverage;
use super::Solid;
use super::coverage::Field;
use super::family::Family;
use super::kinds::{Kind, Kinds, Route, Visit};
use super::program::Uniforms;
use super::programs::Programs;
use super::room::FieldRoom;
use super::texture::{Content, FieldTexture, Grid, TexelPixels};
use crate::Error;
use crate::failure::{DrawFailure, FailedDraw};
use crate::retention::{Frames, Retained};
use lavashell_core::prelude::*;
use lavashell_core::surface::SurfaceId;
use skia_safe::{Canvas, Shader};
use std::cell::RefCell;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// The field programs, the kinds of shapes and the field textures stored for recent frames. The material and the
/// glass lens of one shape read the same stored texture.
pub(crate) struct Fields {
    pub(super) programs: Programs,
    pub(super) kinds: Kinds,
    /// Field textures stored while recent frames draw their shape. A texture is built again only when its shape
    /// changes.
    textures: RefCell<Retained<u64, Stored>>,
}

struct Stored {
    shape: Shape,
    textures: Textures,
}

#[derive(Clone)]
struct Textures {
    exact: FieldTexture,
    lens: Option<FieldTexture>,
}

impl Fields {
    pub fn new(room: &FieldRoom) -> Result<Self, Error> {
        Ok(Self {
            programs: Programs::new(room)?,
            kinds: Kinds::new(room)?,
            textures: RefCell::default(),
        })
    }

    /// Begins a frame of `surface` and drops the textures and the shape geometry that no recent frame used.
    pub fn begin_frame(&mut self, surface: SurfaceId) {
        self.kinds.begin_frame(surface);
        self.textures.get_mut().retain_recent(self.kinds.frames());
    }

    pub fn clear_textures(&mut self) {
        *self.textures.get_mut() = Retained::default();
    }

    pub fn forget_surface(&mut self, surface: SurfaceId) {
        self.kinds.forget_surface(surface);
    }

    pub fn frames(&self) -> &Frames {
        self.kinds.frames()
    }

    pub fn route(&self, shape: &Shape) -> Result<Route, DrawFailure> {
        self.kinds.route(shape)
    }

    /// The coverage of the edge of `shape`. Where the field of `shape` needs a texture, the coverage reads a texture
    /// with one texel per device pixel. Returns None when no part of the shape is visible, like `exact` and `lens`.
    pub fn coverage(
        &self,
        canvas: &Canvas,
        shape: &Shape,
        reach: f64,
    ) -> Result<Option<Coverage<'_>>, FailedDraw> {
        let field = match self.programs.coverage.of(self, shape)? {
            Some(bound) => Field::Analytic(bound),
            None => match self.exact(canvas, shape, reach, TexelPixels::DEVICE_PIXEL)? {
                Some(texture) => Field::Texture(&self.programs.texture_coverage, texture),
                None => return Ok(None),
            },
        };
        Ok(Some(Coverage::new(canvas, shape, field)))
    }

    pub fn exact(
        &self,
        canvas: &Canvas,
        shape: &Shape,
        reach: f64,
        pixels: TexelPixels,
    ) -> Result<Option<FieldTexture>, FailedDraw> {
        Ok(self
            .built(canvas, shape, reach, pixels)?
            .map(|built| built.exact))
    }

    pub fn lens(
        &self,
        canvas: &Canvas,
        shape: &Solid,
        pixels: TexelPixels,
    ) -> Result<Option<FieldTexture>, FailedDraw> {
        let shape: &Shape = shape;
        let Some(built) = self.built(canvas, shape, 0.0, pixels)? else {
            return Ok(None);
        };
        if let Some(lens) = built.lens {
            return Ok(Some(lens));
        }
        let lens = FieldTexture::new(canvas, self, Content::Lens, shape, built.exact.grid())?;
        let key = self.key(shape, built.exact.texel());
        let textures = Textures {
            exact: built.exact,
            lens: Some(lens.clone()),
        };
        self.store(key, shape, textures);
        Ok(Some(lens))
    }

    /// The textures of `shape` that cover its visible part and `reach` dp around it.
    /// Stored textures of the shape that cover a smaller region are rebuilt over the
    /// stored region and the needed region together.
    fn built(
        &self,
        canvas: &Canvas,
        shape: &Shape,
        reach: f64,
        pixels: TexelPixels,
    ) -> Result<Option<Textures>, FailedDraw> {
        let Some(needed) = Grid::around(canvas, shape, reach, pixels)? else {
            return Ok(None);
        };
        let key = self.key(shape, needed.texel());
        let found = self.found(key, shape, needed.texel());
        let grid = match found {
            Some(built) if built.exact.grid().covers(&needed) => return Ok(Some(built)),
            Some(built) => built.exact.grid().joined(&needed, canvas)?,
            None => needed,
        };
        let built = Textures {
            exact: FieldTexture::new(canvas, self, Content::Distance, shape, grid)?,
            lens: None,
        };
        self.store(key, shape, built.clone());
        Ok(Some(built))
    }

    /// The textures stored under `key` for `shape` at `texel`, marked as used by the current frame.
    fn found(&self, key: u64, shape: &Shape, texel: f64) -> Option<Textures> {
        let mut stored = self.textures.borrow_mut();
        let found = stored.get(&key, self.kinds.frames())?;
        (found.shape == *shape && found.textures.exact.texel().to_bits() == texel.to_bits())
            .then(|| found.textures.clone())
    }

    fn store(&self, key: u64, shape: &Shape, textures: Textures) {
        let stored = Stored {
            shape: shape.clone(),
            textures,
        };
        (self.textures.borrow_mut()).insert(key, stored, self.kinds.frames());
    }

    pub(super) fn exact_on(
        &self,
        grid: &Grid,
        shape: &Shape,
    ) -> Result<Option<Shader>, DrawFailure> {
        match self.found(self.key(shape, grid.texel()), shape, grid.texel()) {
            Some(built) if built.exact.grid() == *grid => built.exact.shader().map(Some),
            _ => Ok(None),
        }
    }

    fn key(&self, shape: &Shape, texel: f64) -> u64 {
        let mut hasher = DefaultHasher::new();
        texel.to_bits().hash(&mut hasher);
        self.kinds.hash(shape, &mut hasher);
        hasher.finish()
    }

    /// The texels of the field of `shape` on `grid`. When the geometry of `shape` fits into the uniforms of the texel
    /// program for its kind, that program evaluates the field. Otherwise the kind draws the field, such as by
    /// combining the fields of the members of a union.
    pub(super) fn texels(
        &self,
        content: Content,
        grid: &Grid,
        target: &Canvas,
        shape: &Shape,
    ) -> Result<Shader, FailedDraw> {
        struct Texelled<'a> {
            fields: &'a Fields,
            content: Content,
            grid: &'a Grid,
            target: &'a Canvas,
        }
        impl Visit for Texelled<'_> {
            type Output = Result<Shader, FailedDraw>;
            fn visit<K: Kind>(self, kinds: &Kinds, kind: &K, shape: &K::Shape) -> Self::Output {
                let family = self.fields.programs.texels[self.content].family_of::<K>();
                match kind.fit(kinds, shape, family.map_or(0, Family::most)) {
                    Ok(uniforms) => {
                        let uniforms = (uniforms, ());
                        let family = family.ok_or(DrawFailure::ShaderRejected)?;
                        let program = family.holding(uniforms.columns())?;
                        program.evaluate(&uniforms, ()).map_err(FailedDraw::from)
                    }
                    Err(texels) => {
                        texels.texels(self.fields, self.content, self.grid, self.target, shape)
                    }
                }
            }
        }
        let texelled = Texelled {
            fields: self,
            content,
            grid,
            target,
        };
        self.kinds.visit(shape, texelled)
    }
}
