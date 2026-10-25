// SPDX-License-Identifier: GPL-3.0-only
use super::SurfaceAccess;
use crate::color::channel_step;
use crate::failure::{DrawFailure, FailedDraw};
use crate::field::{
    self, Analytic, Content, FieldRoom, Part, Pass, Program, Solid, TexelPixels, children,
    uniform_block,
};
use crate::paint::{sklength, skrect};
use crate::resources::PaintResources;
use crate::scalar::scalar;
use crate::silhouette::clip_inside;
use lavashell_core::prelude::*;
use skia_safe::{
    Canvas, FilterMode, IRect, Matrix, Paint, RoundOut, SamplingOptions, Shader, TileMode,
    canvas::SaveLayerRec, image_filters,
};

#[derive(Clone, Copy)]
pub(super) enum Clip<'a> {
    /// Inside the drawn shape, holes included.
    Shape(&'a Shape),
    /// No clip of the shape, for a cached image that is clipped to the shape later, when it is composited.
    Bounds,
}

/// A backdrop with its blur, its refraction and its dispersion halved `level` times by `BlurLevel::new`.
#[derive(Clone, Copy)]
pub(super) struct BlurLevel {
    pub effect: Backdrop,
}

impl BlurLevel {
    pub fn new(effect: &Backdrop, level: u8) -> Self {
        let mut reduced = *effect;
        reduced.blur = effect.blur.halved(level);
        if let Some(glass) = &mut reduced.glass {
            glass.refraction = glass.refraction.halved(level);
            glass.dispersion = glass.dispersion.halved(level);
        }
        Self { effect: reduced }
    }
}

uniform_block!(Optics {
    refraction: 1,
    dispersion: 1,
    rim: 1,
});

impl Optics {
    fn new(glass: Glass) -> Self {
        Self {
            refraction: [glass.refraction.get().get()],
            dispersion: [glass.dispersion.get().get()],
            rim: [glass.rim.get()],
        }
    }
}

/// The shader part that draws the glass. It reads the lens field computed by the shader part before it and
/// refracts the `backdrop` child through that field.
const OPTICS: Part = Part::new(include_str!("../../glass.sksl"));

children!(Surface { backdrop });

children!(
    /// The textures read by the glass shader when the field of the shape comes from a texture. `field` has the
    /// signed distance in its red channel, `lens` has the lens field and `backdrop` has the surface below the
    /// glass. Each texture maps to dp coordinates.
    LensTextures {
        field,
        lens,
        backdrop,
    }
);

pub(super) struct GlassPrograms {
    analytic: Analytic<Optics, Surface>,
    texture: Program<Optics, LensTextures>,
}

impl GlassPrograms {
    pub fn new(room: &FieldRoom) -> Result<Self, crate::Error> {
        Ok(Self {
            analytic: Analytic::new("glass", Content::Lens, &[OPTICS], Pass::Own(room))?,
            texture: Program::shader("glass on a field texture", &[field::LENS_TEXTURE, OPTICS])?,
        })
    }

    pub fn backdrop(
        &self,
        canvas: &Canvas,
        resources: &PaintResources,
        shape: &Shape,
        level: BlurLevel,
        access: SurfaceAccess,
    ) -> Result<(), FailedDraw> {
        let optics = Solid::of(shape);
        self.apply(
            canvas,
            resources,
            &optics,
            level,
            Clip::Shape(shape),
            access,
        )
    }

    pub fn apply(
        &self,
        canvas: &Canvas,
        resources: &PaintResources,
        optics: &Solid,
        level: BlurLevel,
        clip: Clip,
        access: SurfaceAccess,
    ) -> Result<(), FailedDraw> {
        let effect = level.effect;
        let bounds = skrect(optics.bounds())?;
        let sigma = sklength(effect.blur)?;
        let mut filter = image_filters::blur((sigma, sigma), TileMode::Clamp, None, None);
        let mut lens = Ok(());
        if let Some(glass) = effect.glass {
            match self.analytic.of(&resources.fields, optics)? {
                Some(bound) => {
                    filter =
                        Some(bound.filter(&Optics::new(glass), glass.reach().get(), filter)?);
                }
                // A backdrop filter gives its shader only one child, the backdrop. Glass that needs field
                // textures is therefore drawn from a snapshot of the surface, with the textures as further children.
                None if access != SurfaceAccess::Unreadable
                    && axis_aligned(&canvas.local_to_device_as_3x3()) =>
                {
                    let Some(shader) =
                        self.sampled_surface(canvas, resources, optics, level, glass)?
                    else {
                        return Ok(());
                    };
                    return within(canvas, resources, clip, || {
                        let mut paint = Paint::default();
                        let _chained_paint = paint
                            .set_shader(shader)
                            .set_blend_mode(skia_safe::BlendMode::Src);
                        let _chained_canvas = canvas.draw_rect(bounds, &paint);
                    });
                }
                None => lens = Err(DrawFailure::LensWithoutSurface.into()),
            }
        }
        if let Some(filter) = filter {
            within(canvas, resources, clip, || {
                // The layer starts with the filtered pixels of the parent. Restoring it with Src replaces that
                // region. SrcOver would count the alpha twice.
                let mut paint = Paint::default();
                let _chained_paint = paint.set_blend_mode(skia_safe::BlendMode::Src);
                let _save_count = canvas.save_layer(
                    &SaveLayerRec::default()
                        .bounds(&bounds)
                        .paint(&paint)
                        .backdrop(&filter)
                        .backdrop_tile_mode(TileMode::Clamp),
                );
                let _chained_canvas = canvas.restore();
            })?;
        }
        lens
    }

    /// The shader of glass whose lens field comes from a field texture of `optics` and which refracts a blurred
    /// snapshot of the surface. Returns `None` when no part of `optics` is visible.
    fn sampled_surface(
        &self,
        canvas: &Canvas,
        resources: &PaintResources,
        optics: &Solid,
        level: BlurLevel,
        glass: Glass,
    ) -> Result<Option<Shader>, FailedDraw> {
        let matrix = canvas.local_to_device_as_3x3();
        let step =
            channel_step(canvas.image_info().color_type()).ok_or(DrawFailure::SurfaceRead)?;
        let (reach, sigma) = (level.effect.reach(step).get(), sklength(level.effect.blur)?);
        let Some(backdrop) = blurred_surface(canvas, matrix, optics, reach, sigma)? else {
            return Ok(None);
        };
        // The snapshot above exists, so the canvas has pixels.
        let texel = TexelPixels::lens(canvas).ok_or(DrawFailure::SurfaceRead)?;
        let Some(lens) = resources.fields.lens(canvas, optics, texel)? else {
            return Ok(None);
        };
        let Some(exact) = resources.fields.exact(canvas, optics, 0.0, texel)? else {
            return Ok(None);
        };
        let textures = LensTextures {
            field: exact.shader()?,
            lens: lens.shader()?,
            backdrop,
        };
        self.texture
            .evaluate(&Optics::new(glass), textures)
            .map(Some)
            .map_err(FailedDraw::from)
    }
}

pub(super) fn within(
    canvas: &Canvas,
    resources: &PaintResources,
    clip: Clip,
    draw: impl FnOnce(),
) -> Result<(), FailedDraw> {
    let saved = canvas.save();
    let clipped = match clip {
        Clip::Shape(shape) => clip_inside(canvas, resources, shape),
        Clip::Bounds => Ok(()),
    };
    if clipped.is_ok() {
        draw();
    }
    let _chained_canvas = canvas.restore_to_count(saved);
    clipped
}

/// A shader in local coordinates with the surface within `reach` dp around the bounds of `shape`, blurred with
/// `sigma` dp. Returns `None` when no part of that region is inside the device clip.
fn blurred_surface(
    canvas: &Canvas,
    matrix: Matrix,
    shape: &Shape,
    reach: f64,
    sigma: f32,
) -> Result<Option<Shader>, DrawFailure> {
    let Some(capture) = capture_bounds(canvas, matrix, shape.bounds(), reach)? else {
        return Ok(None);
    };
    let snapshot = canvas
        .snapshot_surface(Some(capture))
        .ok_or(DrawFailure::SurfaceRead)?;
    let mut blurred = canvas
        .new_surface(&canvas.image_info().with_dimensions(capture.size()), None)
        .ok_or(DrawFailure::Allocation)?;
    let mut paint = Paint::default();
    let _chained_paint = paint
        .set_blend_mode(skia_safe::BlendMode::Src)
        .set_image_filter(image_filters::blur(
            (sigma * matrix.scale_x(), sigma * matrix.scale_y()),
            TileMode::Clamp,
            None,
            None,
        ));
    let _chained_canvas = blurred.canvas().draw_image(&snapshot, (0, 0), Some(&paint));
    let inverse = matrix.invert().ok_or(DrawFailure::SingularTransform)?;
    let placement = Matrix::concat(
        &inverse,
        &Matrix::translate(skia_safe::Rect::from(capture).tl()),
    );
    let shader = blurred.image_snapshot().to_shader(
        (TileMode::Clamp, TileMode::Clamp),
        SamplingOptions::from(FilterMode::Linear),
        &placement,
    );
    shader.ok_or(DrawFailure::ShaderRejected).map(Some)
}

/// The device pixels covered by `rect`, grown by `reach` dp on each side and mapped by `matrix`, limited to the
/// device clip of `canvas`. `None` when that area lies outside the clip.
pub(super) fn capture_bounds(
    canvas: &Canvas,
    matrix: Matrix,
    rect: Rect,
    reach: f64,
) -> Result<Option<IRect>, DrawFailure> {
    let reach = scalar(reach)?;
    let mapped: IRect = matrix
        .map_rect(skrect(rect)?.with_outset((reach, reach)))
        .0
        .round_out();
    Ok(canvas
        .device_clip_bounds()
        .and_then(|clip| IRect::intersect(&mapped, &clip)))
}

/// Whether `matrix` only scales by positive factors and moves. A snapshot of the surface then maps onto local
/// coordinates by a scale and an offset.
pub(super) fn axis_aligned(matrix: &Matrix) -> bool {
    matrix.skew_x() == 0.0
        && matrix.skew_y() == 0.0
        && matrix.scale_x() > 0.0
        && matrix.scale_y() > 0.0
        && !matrix.has_perspective()
}
