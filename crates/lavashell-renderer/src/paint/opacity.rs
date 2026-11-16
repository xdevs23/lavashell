// SPDX-License-Identifier: GPL-3.0-only
use crate::destination::GroupOpacity;
use crate::error::{Error, SkslDiagnostic};
use crate::failure::{DrawFailure, Failures};
use crate::scalar::scalar;
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
        opacity: GroupOpacity,
        bounds: Option<Rect>,
        reads_parent: bool,
        failures: &Failures,
    ) -> Result<(), DrawFailure> {
        let GroupOpacity::Translucent { alpha } = opacity else {
            let _restored_by_the_caller = canvas.save();
            return Ok(());
        };
        let alpha = scalar(alpha)?;
        if reads_parent {
            let alpha = if alpha.is_finite() {
                alpha.max(0.0)
            } else {
                0.0
            };
            let uniforms = Data::new_copy(&alpha.to_ne_bytes());
            let Some(blender) = self.interpolate.make_blender(uniforms, None) else {
                failures.record(DrawFailure::ShaderRejected);
                let _restored_by_the_caller = canvas.save_layer_alpha_f(bounds, alpha);
                return Ok(());
            };
            let mut paint = Paint::default();
            let _paint_returned_for_chaining = paint.set_blender(blender);
            super::parent_copy::save(canvas, &paint, None);
        } else {
            let _restored_by_the_caller = canvas.save_layer_alpha_f(bounds, alpha);
        }
        Ok(())
    }
}
