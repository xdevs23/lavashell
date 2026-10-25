// SPDX-License-Identifier: GPL-3.0-only
use super::EDGE_PIXELS;
use crate::paint::{device_scale, max_image_side};
use skia_safe::{Canvas, IRect, ISize, Matrix, Rect, RoundOut, Size, textlayout::Paragraph};

/// The union of the bounds of the glyphs of `paragraph`, relative to its origin.
pub(crate) fn ink_bounds(paragraph: &mut Paragraph) -> Rect {
    let mut ink = Rect::new_empty();
    paragraph.visit(|_, run| {
        let Some(run) = run else {
            return;
        };
        let mut bounds = vec![Rect::new_empty(); run.count()];
        run.font().get_bounds(run.glyphs(), &mut bounds, None);
        for (glyph, position) in bounds.iter().zip(run.positions()) {
            ink.join(glyph.with_offset(run.origin() + *position));
        }
    });
    ink
}

pub(crate) struct ParagraphTile {
    pub scale: f32,
    pub phase: (f32, f32),
    pub size: ISize,
    pub bytes: usize,
    pub destination: Rect,
    pub paint_origin: (f32, f32),
}

impl ParagraphTile {
    pub fn clipped(mut self, clip: Rect, bytes_per_pixel: usize) -> Option<Self> {
        let mut bounds = self.destination;
        if !bounds.intersect(clip) {
            return None;
        }
        let from_tile = |left: f32, top: f32| {
            (
                (left - self.destination.left) * self.scale,
                (top - self.destination.top) * self.scale,
            )
        };
        let (left, top) = from_tile(bounds.left, bounds.top);
        let (right, bottom) = from_tile(bounds.right, bounds.bottom);
        let pixels = IRect::intersect(
            &Rect::new(left, top, right, bottom).round_out(),
            &IRect::from_size(self.size),
        )?;
        let kept = Rect::from_irect(pixels);
        self.size = pixels.size();
        self.destination = Rect::from_xywh(
            self.destination.left + kept.left / self.scale,
            self.destination.top + kept.top / self.scale,
            kept.width() / self.scale,
            kept.height() / self.scale,
        );
        self.paint_origin.0 -= kept.left / self.scale;
        self.paint_origin.1 -= kept.top / self.scale;
        self.bytes = pixel_bytes(self.size, bytes_per_pixel)?;
        Some(self)
    }

    /// A tile of `ink` at the largest scale of the transform of `canvas`. Where the transform is exactly that scale
    /// and a translation, the tile has the subpixel phase of `origin` on the device. Otherwise the phase is zero.
    pub fn new(
        canvas: &Canvas,
        ink: Rect,
        origin: (f32, f32),
        bytes_per_pixel: usize,
        max_bytes: usize,
    ) -> Option<Self> {
        let matrix = canvas.local_to_device_as_3x3();
        let scale = device_scale(canvas)?;
        if !scale.is_finite() {
            return None;
        }
        let translate = (matrix.translate_x(), matrix.translate_y());
        let phase = if matrix == Matrix::scale_translate((scale, scale), translate) {
            (
                (origin.0 * scale + translate.0).rem_euclid(1.0) / scale,
                (origin.1 * scale + translate.1).rem_euclid(1.0) / scale,
            )
        } else {
            (0.0, 0.0)
        };
        Self::placed(
            canvas,
            ink,
            origin,
            scale,
            phase,
            bytes_per_pixel,
            max_bytes,
        )
    }

    /// The scale of `canvas` when it maps local units onto whole device pixels, without skew,
    /// perspective or a fractional offset.
    pub fn integer_scale(canvas: &Canvas) -> Option<f32> {
        let matrix = canvas.local_to_device_as_3x3();
        let scale = matrix.scale_x();
        let translate = (matrix.translate_x(), matrix.translate_y());
        (scale.is_finite()
            && scale > 0.0
            && scale.fract() == 0.0
            && translate.0.fract() == 0.0
            && translate.1.fract() == 0.0
            && matrix == Matrix::scale_translate((scale, scale), translate))
        .then_some(scale)
    }

    pub fn at_integer_scale(
        canvas: &Canvas,
        ink: Rect,
        origin: (f32, f32),
        scale: f32,
        bytes_per_pixel: usize,
        max_bytes: usize,
    ) -> Option<Self> {
        let phase = (
            (origin.0 * scale).rem_euclid(1.0) / scale,
            (origin.1 * scale).rem_euclid(1.0) / scale,
        );
        Self::placed(
            canvas,
            ink,
            origin,
            scale,
            phase,
            bytes_per_pixel,
            max_bytes,
        )
    }

    /// A tile that starts on a whole device pixel at least `EDGE_PIXELS` before the ink and ends at least
    /// `EDGE_PIXELS` after it. Returns None when the tile exceeds the longest image side or `max_bytes`.
    fn placed(
        canvas: &Canvas,
        ink: Rect,
        origin: (f32, f32),
        scale: f32,
        phase: (f32, f32),
        bytes_per_pixel: usize,
        max_bytes: usize,
    ) -> Option<Self> {
        let before = |ink_start: f32| (EDGE_PIXELS - ink_start * scale).ceil() / scale;
        let lead = (before(ink.left), before(ink.top));
        let after =
            |lead: f32, phase: f32, ink_end: f32| (lead + phase + ink_end) * scale + EDGE_PIXELS;
        let size = Size::new(
            after(lead.0, phase.0, ink.right),
            after(lead.1, phase.1, ink.bottom),
        )
        .to_ceil();
        let max_side = max_image_side(canvas);
        if size.is_empty() || size.width > max_side || size.height > max_side {
            return None;
        }
        let bytes = pixel_bytes(size, bytes_per_pixel).filter(|bytes| *bytes <= max_bytes)?;
        let extent = Size::from(size);
        Some(Self {
            scale,
            phase,
            size,
            bytes,
            paint_origin: (lead.0 + phase.0, lead.1 + phase.1),
            destination: Rect::from_xywh(
                origin.0 - lead.0 - phase.0,
                origin.1 - lead.1 - phase.1,
                extent.width / scale,
                extent.height / scale,
            ),
        })
    }
}

/// The bytes of an image of `size` pixels, or None when they do not fit in memory.
pub(crate) fn pixel_bytes(size: ISize, bytes_per_pixel: usize) -> Option<usize> {
    let width = usize::try_from(size.width).ok()?;
    let height = usize::try_from(size.height).ok()?;
    width.checked_mul(height)?.checked_mul(bytes_per_pixel)
}
