// SPDX-License-Identifier: GPL-3.0-only
use super::{ACCENT, INK, LINE, MUTED, data, label};
use crate::{CopyKey, copy, model::Demo};
use lavashell::prelude::*;
use std::ops::{Div, Mul};

fn slider<T>(
    ui: &mut Ui,
    key: CopyKey,
    state: &State<T>,
    max: T,
    enabled: bool,
    modifier: Modifier,
    format: impl Fn(T) -> String,
) where
    T: Copy + Div<Output = f64> + Mul<f64, Output = T> + 'static,
{
    let value = state.get();
    let own = Modifier::new()
        .gap(10.dp())
        .fill_max_width()
        .opacity(if enabled { 1.0 } else { 0.4 });
    ui.column(own.then(modifier), |ui| {
        ui.row(Modifier::new(), |ui| {
            label(ui, key, 13.sp(), INK);
            ui.space(Modifier::new());
            data(ui, format(value), 12.sp(), MUTED);
        });
        let (read, state) = (state.clone(), state.clone());
        ui.slider(
            move || read.get() / max,
            None,
            Modifier::new()
                .tag(key.to_string())
                .accessible_name(copy(key))
                .enabled(enabled)
                .fill_max_width()
                .height(24.dp()),
            move |v| state.set(max * v),
            |ui, value| {
                ui.canvas(
                    Size::new(260.dp(), 20.dp()),
                    Modifier::new().fill_max_width(),
                    move |d| {
                        let value = value();
                        let w = d.size().width;
                        d.fill(
                            Shape::rounded(Rect::new(0.dp(), 8.dp(), w, 3.dp()), 2.dp()),
                            LINE,
                        );
                        d.fill(
                            Shape::rounded(Rect::new(0.dp(), 8.dp(), w * value, 3.dp()), 2.dp()),
                            ACCENT,
                        );
                        d.circle(
                            Point::new((w * value).clamp(6.dp(), w - 6.dp()), 9.5.dp()),
                            6.dp(),
                            ACCENT,
                        );
                    },
                );
            },
        );
    });
}

/// Composes the controls of the treatment parameters, with smaller gaps between them when the window is `compact`.
pub(super) fn controls(ui: &mut Ui, demo: &Demo, compact: bool) {
    ui.column(
        Modifier::new()
            .gap(if compact { 8.dp() } else { 23.dp() })
            .width(276.dp())
            .padding_xy(0.dp(), 12.dp()),
        |ui| {
            label(ui, CopyKey::Parameters, 16.sp(), INK);
            let parameters = demo.treatment.get().parameters();
            slider(
                ui,
                CopyKey::Blur,
                &demo.blur,
                32.dp(),
                parameters.blur.is_some(),
                Modifier::new(),
                |v| format!("{:.1} dp", v.get()),
            );
            slider(
                ui,
                CopyKey::Opacity,
                &demo.opacity,
                1.0,
                true,
                Modifier::new(),
                |v| format!("{:.0}%", v * 100.0),
            );
            slider(
                ui,
                CopyKey::Radius,
                &demo.radius,
                64.dp(),
                true,
                Modifier::new(),
                |v| format!("{:.0} dp", v.get()),
            );
            slider(
                ui,
                CopyKey::Refraction,
                &demo.refraction,
                48.dp(),
                parameters.refraction.is_some(),
                Modifier::new(),
                |v| format!("{:.0} dp", v.get()),
            );
            let state = demo.clone();
            ui.button(
                Modifier::new()
                    .tag("reset")
                    .fill_max_width()
                    .border(LINE, 1.dp(), 10.dp())
                    .clip(10.dp())
                    .padding(12.dp()),
                move || state.reset(),
                |ui| label(ui, CopyKey::Reset, 13.sp(), INK),
            );
        },
    );
}

pub(super) fn palette(ui: &mut Ui, demo: &Demo) {
    ui.column(Modifier::new().gap(16.dp()).weight(1.0), |ui| {
        label(ui, CopyKey::Palette, 16.sp(), INK);
        ui.row(Modifier::new().gap(28.dp()), |ui| {
            let weighted = || Modifier::new().weight(1.0);
            slider(ui, CopyKey::Hue, &demo.hue, 360.0, true, weighted(), |v| {
                format!("{v:.0}°")
            });
            slider(
                ui,
                CopyKey::Chroma,
                &demo.chroma,
                0.25,
                true,
                weighted(),
                |v| format!("{v:.3}"),
            );
        });
        let palette = TonalPalette::new(demo.hue.get(), demo.chroma.get());
        ui.row(Modifier::new().gap(7.dp()), |ui| {
            for l in [0.20, 0.30, 0.40, 0.50, 0.60, 0.70, 0.80, 0.90, 0.96] {
                ui.column(Modifier::new().gap(7.dp()).weight(1.0), |ui| {
                    ui.spacer(
                        0.dp(),
                        36.dp(),
                        Modifier::new()
                            .fill_max_width()
                            .clip(7.dp())
                            .background(palette.tone(l).mapped(Gamut::Srgb)),
                    );
                    data(ui, format!("{l:.2}"), 11.sp(), MUTED);
                });
            }
        });
    });
}

pub(super) fn blending(ui: &mut Ui, demo: &Demo) {
    ui.column(Modifier::new().gap(16.dp()).width(276.dp()), |ui| {
        label(ui, CopyKey::Blending, 16.sp(), INK);
        ui.row(Modifier::new().gap(16.dp()), |ui| {
            for (mode, key) in [
                (BlendMode::Multiply, CopyKey::BlendMultiply),
                (BlendMode::Screen, CopyKey::BlendScreen),
                (BlendMode::Difference, CopyKey::BlendDifference),
            ] {
                let state = demo.clone();
                let selected = demo.blend.get() == mode;
                ui.button(
                    Modifier::new()
                        .tag(key.to_string())
                        .selected(selected)
                        .padding_xy(0.dp(), 3.dp()),
                    move || state.blend.set(mode),
                    |ui| label(ui, key, 12.sp(), if selected { ACCENT } else { MUTED }),
                );
            }
        });
        let mode = demo.blend.get();
        ui.canvas(Size::new(276.dp(), 90.dp()), Modifier::new(), move |d| {
            d.fill(
                Shape::rounded(Rect::new(0.dp(), 0.dp(), d.size().width, 90.dp()), 12.dp()),
                Color::oklch(0.93, 0.014, 80.0),
            );
            d.circle(
                Point::new(103.dp(), 45.dp()),
                35.dp(),
                Color::oklch(0.74, 0.18, 30.0).mapped(Gamut::Srgb),
            );
            let bounds = Rect::new(0.dp(), 0.dp(), 276.dp(), 90.dp());
            d.effect_layer(bounds, mode, NonNegativeDp::ZERO);
            d.circle(
                Point::new(151.dp(), 45.dp()),
                35.dp(),
                Color::oklch(0.72, 0.16, 265.0).mapped(Gamut::Srgb),
            );
            d.restore();
        });
    });
}

/// The copy key of the label for `profile`.
fn profile_key(profile: OutputProfile) -> CopyKey {
    match profile {
        OutputProfile::Srgb => CopyKey::ProfileSrgb,
        OutputProfile::DisplayP3 => CopyKey::ProfileP3,
        OutputProfile::Rec2020 => CopyKey::ProfileRec2020,
        OutputProfile::Hdr10 => CopyKey::ProfileHdr10,
        OutputProfile::ScRgb => CopyKey::ProfileScrgb,
    }
}

pub(super) fn output(ui: &mut Ui, demo: &Demo) {
    label(ui, CopyKey::Output, 12.sp(), MUTED);
    ui.row(
        Modifier::new()
            .align_items(Align::Center)
            .tag("output-info")
            .fill_max_width()
            .border(LINE, 1.dp(), 12.dp())
            .padding(18.dp()),
        |ui| {
            ui.column(Modifier::new().gap(5.dp()), |ui| {
                label(ui, CopyKey::Requested, 13.sp(), INK);
                label(ui, profile_key(demo.request.get().profile), 12.sp(), ACCENT);
            });
            ui.space(Modifier::new());
            ui.column(Modifier::new().gap(5.dp()), |ui| {
                label(ui, CopyKey::Active, 12.sp(), MUTED);
                match demo.output.get() {
                    Some(info) => label(ui, profile_key(info.profile), 13.sp(), INK),
                    None => data(ui, "—", 13.sp(), INK),
                }
            });
            ui.spacer(28.dp(), 0.dp(), Modifier::new());
            ui.column(Modifier::new().gap(5.dp()), |ui| {
                label(ui, CopyKey::Precision, 12.sp(), MUTED);
                data(
                    ui,
                    demo.output.get().map_or("—".into(), |i| {
                        format!("{} / α {}", i.color_bits, i.alpha_bits)
                    }),
                    13.sp(),
                    INK,
                );
            });
        },
    );
    if demo.output.get().is_some_and(|i| i.fallback) {
        label(ui, CopyKey::Fallback, 12.sp(), ACCENT);
    }
}
