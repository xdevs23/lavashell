// SPDX-License-Identifier: GPL-3.0-only
use super::{ANTIALIASING_PIXELS, EDGE_PIXELS};
use crate::paint::{device_scale, max_image_side};
use skia_safe::{
    Canvas, IRect, ISize, Matrix, Point, Rect, RoundOut, Size, Vector, textlayout::Paragraph,
};

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

pub(crate) fn outset_by_antialiasing(ink: Rect, device_scale: f32) -> Rect {
    let margin = ANTIALIASING_PIXELS / device_scale;
    ink.with_outset((margin, margin))
}

#[derive(Clone, Copy)]
pub(crate) struct TileBudget {
    pub bytes_per_pixel: usize,
    pub max_bytes: usize,
}

pub(crate) struct ParagraphTile {
    pub scale: f32,
    pub phase: Vector,
    pub size: ISize,
    pub bytes: usize,
    pub destination: Rect,
    pub paint_origin: Point,
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
        self.paint_origin -= Vector::new(kept.left / self.scale, kept.top / self.scale);
        self.bytes = pixel_bytes(self.size, bytes_per_pixel)?;
        Some(self)
    }

    pub fn new(canvas: &Canvas, ink: Rect, origin: Point, budget: TileBudget) -> Option<Self> {
        let scale = device_scale(canvas)?;
        if !scale.is_finite() {
            return None;
        }
        let phase = pixel_aligned_translate(canvas, scale).map_or(Vector::default(), |translate| {
            pixel_phase(origin, scale, translate)
        });
        Self::placed(canvas, ink, origin, scale, phase, budget)
    }

    pub fn integer_scale(canvas: &Canvas) -> Option<f32> {
        let scale = canvas.local_to_device_as_3x3().scale_x();
        let translate = pixel_aligned_translate(canvas, scale)?;
        (scale.is_finite()
            && scale > 0.0
            && scale.fract() == 0.0
            && translate.x.fract() == 0.0
            && translate.y.fract() == 0.0)
            .then_some(scale)
    }

    pub fn at_integer_scale(
        canvas: &Canvas,
        ink: Rect,
        origin: Point,
        scale: f32,
        budget: TileBudget,
    ) -> Option<Self> {
        let phase = pixel_phase(origin, scale, Vector::default());
        Self::placed(canvas, ink, origin, scale, phase, budget)
    }

    fn placed(
        canvas: &Canvas,
        ink: Rect,
        origin: Point,
        scale: f32,
        phase: Vector,
        budget: TileBudget,
    ) -> Option<Self> {
        let before = |ink_start: f32| (EDGE_PIXELS - ink_start * scale).ceil() / scale;
        let lead = Vector::new(before(ink.left), before(ink.top));
        let after =
            |lead: f32, phase: f32, ink_end: f32| (lead + phase + ink_end) * scale + EDGE_PIXELS;
        let size = Size::new(
            after(lead.x, phase.x, ink.right),
            after(lead.y, phase.y, ink.bottom),
        )
        .to_ceil();
        let max_side = max_image_side(canvas);
        if size.is_empty() || size.width > max_side || size.height > max_side {
            return None;
        }
        let bytes =
            pixel_bytes(size, budget.bytes_per_pixel).filter(|bytes| *bytes <= budget.max_bytes)?;
        let extent = Size::from(size);
        Some(Self {
            scale,
            phase,
            size,
            bytes,
            paint_origin: lead + phase,
            destination: Rect::from_point_and_size(
                origin - lead - phase,
                (extent.width / scale, extent.height / scale),
            ),
        })
    }
}

pub(super) fn pixel_aligned_translate(canvas: &Canvas, scale: f32) -> Option<Vector> {
    let matrix = canvas.local_to_device_as_3x3();
    let translate = Vector::new(matrix.translate_x(), matrix.translate_y());
    (matrix == Matrix::scale_translate((scale, scale), translate)).then_some(translate)
}

pub(super) fn pixel_phase(local: Point, scale: f32, translate: Vector) -> Vector {
    let phase = |local: f32, translate: f32| (local * scale + translate).rem_euclid(1.0) / scale;
    Vector::new(phase(local.x, translate.x), phase(local.y, translate.y))
}

pub(crate) fn pixel_bytes(size: ISize, bytes_per_pixel: usize) -> Option<usize> {
    let width = usize::try_from(size.width).ok()?;
    let height = usize::try_from(size.height).ok()?;
    width.checked_mul(height)?.checked_mul(bytes_per_pixel)
}
