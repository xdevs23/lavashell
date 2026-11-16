// SPDX-License-Identifier: GPL-3.0-only
use crate::destination::Destination;
use crate::error::{Error, SkslDiagnostic};
use crate::failure::{DrawFailure, Failures};
use skia_safe::{Canvas, Data, Paint, Rect, RuntimeEffect};

pub(super) struct Opacity {
    interpolate: RuntimeEffect,
}

impl Opacity {
    pub fn new() -> Result<Self, Error> {
        Ok(Self {
            interpolate: RuntimeEffect::make_for_blender(
                "uniform float alpha; half4 main(half4 src, half4 dst) { return mix(dst, src, alpha); }",
                None,
            )
            .map_err(|diagnostic| Error::CompileProgram {
                program: "opacity".to_owned(),
                source: SkslDiagnostic(diagnostic),
            })?,
        })
    }

    pub fn save(
        &self,
        canvas: &Canvas,
        alpha: f32,
        bounds: Option<Rect>,
        reads_parent: bool,
        failures: &Failures,
    ) {
        if Destination::for_opacity(f64::from(alpha)) == Destination::Surface {
            let _save_count = canvas.save();
            return;
        }
        if reads_parent {
            let alpha = if alpha.is_finite() {
                alpha.max(0.0)
            } else {
                0.0
            };
            let uniforms = Data::new_copy(&alpha.to_ne_bytes());
            let Some(blender) = self.interpolate.make_blender(uniforms, None) else {
                failures.record(DrawFailure::ShaderRejected);
                let _save_count = canvas.save_layer_alpha_f(bounds, alpha);
                return;
            };
            let mut paint = Paint::default();
            let _chained_paint = paint.set_blender(blender);
            super::parent_copy::save(canvas, &paint, None);
        } else {
            let _save_count = canvas.save_layer_alpha_f(bounds, alpha);
        }
    }
}
