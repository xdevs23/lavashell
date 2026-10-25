// SPDX-License-Identifier: GPL-3.0-only
//! The cache key of a tile: what the tile contains and the pixel format it was rasterized in.

use super::path::PathDrawing;
use skia_safe::{AlphaType, ColorSpace, ColorType, ImageInfo, PixelGeometry, SurfaceProps};
use std::hash::{Hash, Hasher};
use std::rc::Rc;

/// What a paragraph tile contains.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ParagraphRaster {
    /// The paragraph as Skia paints it.
    Native,
    /// The RGB coverage of the paragraph on an opaque destination.
    OpaqueCoverage,
    /// The coverage masks of the paragraph on a translucent destination.
    TranslucentCoverage,
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) enum TileContent {
    /// The paragraph with the cache key `paragraph`, at the scale and the subpixel phase stored as bits.
    Paragraph {
        paragraph: u64,
        raster: ParagraphRaster,
        scale: u32,
        phase: [u32; 2],
    },
    /// The device pixels of a drawn path, starting at the pixel `start`.
    Path {
        drawing: Rc<PathDrawing>,
        start: (i32, i32),
    },
}

impl TileContent {
    pub fn paragraph(
        paragraph: u64,
        raster: ParagraphRaster,
        scale: f32,
        phase: (f32, f32),
    ) -> Self {
        Self::Paragraph {
            paragraph,
            raster,
            scale: scale.to_bits(),
            phase: [phase.0.to_bits(), phase.1.to_bits()],
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
struct SurfaceTraits {
    geometry: PixelGeometry,
    flags: u32,
    text_gamma: u32,
    text_contrast: u32,
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

/// Equal color spaces have equal gamut and transfer function hashes.
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
            text_gamma: props.text_gamma().to_bits(),
            text_contrast: props.text_contrast().to_bits(),
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
