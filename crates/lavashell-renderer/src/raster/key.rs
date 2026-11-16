// SPDX-License-Identifier: GPL-3.0-only

use super::path::PathDrawing;
use skia_safe::{AlphaType, ColorSpace, ColorType, ImageInfo, PixelGeometry, SurfaceProps, Vector};
use std::hash::{Hash, Hasher};
use std::rc::Rc;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ParagraphRaster {
    Native,
    OpaqueCoverage,
    TranslucentCoverage,
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) enum TileContent {
    Paragraph {
        paragraph: u64,
        raster: ParagraphRaster,
        scale: ExactF32,
        phase: [ExactF32; 2],
    },
    Path {
        drawing: Rc<PathDrawing>,
        start: (i32, i32),
    },
}

impl TileContent {
    pub fn paragraph(paragraph: u64, raster: ParagraphRaster, scale: f32, phase: Vector) -> Self {
        Self::Paragraph {
            paragraph,
            raster,
            scale: scale.into(),
            phase: [phase.x.into(), phase.y.into()],
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ExactF32(u32);

impl From<f32> for ExactF32 {
    fn from(value: f32) -> Self {
        Self(value.to_bits())
    }
}

#[derive(Clone, PartialEq, Eq)]
struct SurfaceTraits {
    geometry: PixelGeometry,
    flags: u32,
    text_gamma: ExactF32,
    text_contrast: ExactF32,
}

impl Hash for SurfaceTraits {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(&self.geometry).hash(state);
        self.flags.hash(state);
        self.text_gamma.hash(state);
        self.text_contrast.hash(state);
    }
}

#[derive(Clone, PartialEq)]
struct TileColorSpace(ColorSpace);

impl Eq for TileColorSpace {}

impl Hash for TileColorSpace {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.to_xyzd50_hash().0.hash(state);
        self.0.transfer_fn_hash().hash(state);
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct TileFormat {
    color_type: ColorType,
    alpha_type: AlphaType,
    size: (i32, i32),
    surface: Option<SurfaceTraits>,
    color_space: Option<TileColorSpace>,
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub(super) struct TileKey {
    content: TileContent,
    format: TileFormat,
}

impl TileKey {
    pub fn new(content: TileContent, info: &ImageInfo, props: Option<&SurfaceProps>) -> Self {
        let surface = props.map(|props| SurfaceTraits {
            geometry: props.pixel_geometry(),
            flags: props.flags().bits(),
            text_gamma: props.text_gamma().into(),
            text_contrast: props.text_contrast().into(),
        });
        let color_space = info.color_space().map(TileColorSpace);
        Self {
            content,
            format: TileFormat {
                color_type: info.color_type(),
                alpha_type: info.alpha_type(),
                size: (info.width(), info.height()),
                surface,
                color_space,
            },
        }
    }
}
