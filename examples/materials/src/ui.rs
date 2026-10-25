// SPDX-License-Identifier: GPL-3.0-only
mod controls;

use crate::{
    CopyKey, copy,
    model::{Demo, PANEL, inside},
};
use controls::{blending, controls, output, palette};
use lavashell::{materials::SurfaceTreatment, prelude::*};
use lavashell_example_support::{BODY, Breakpoint, DISPLAY};

const SCALE: TonalPalette = TonalPalette::new(80.0, 0.018);
const BG: Color = SCALE.tone(0.17);
const INK: Color = SCALE.tone(0.95);
const MUTED: Color = SCALE.tone(0.66);
const LINE: Color = SCALE.tone(0.31);
const ACCENT: Color = Color::oklch(0.84, 0.11, 80.0);

fn focus_border(color: Color) -> BorderStyle {
    BorderStyle {
        color,
        width: constant!(NonNegativeDp::try_new(Dp::new(2.0))),
        radius: 10.dp(),
    }
}

fn label(ui: &mut Ui, key: CopyKey, size: Sp, color: Color) {
    ui.text(
        copy(key),
        Modifier::new()
            .font_family(BODY)
            .font_size(size)
            .color(color),
    );
}

fn data(ui: &mut Ui, value: impl Into<String>, size: Sp, color: Color) {
    ui.text(
        value,
        Modifier::new()
            .font_family(BODY)
            .font_size(size)
            .color(color),
    );
}

fn key(t: SurfaceTreatment) -> CopyKey {
    match t {
        SurfaceTreatment::LiquidGlass => CopyKey::Liquid,
        SurfaceTreatment::FrostedGlass => CopyKey::Frosted,
        SurfaceTreatment::Translucent => CopyKey::Translucent,
        SurfaceTreatment::Paper => CopyKey::Paper,
        SurfaceTreatment::Ceramic => CopyKey::Ceramic,
        SurfaceTreatment::BrushedMetal => CopyKey::Metal,
    }
}

pub fn compose(ui: &mut Ui, demo: &Demo) {
    ui.provide(
        &INTERACTION_STYLE,
        InteractionStyle {
            focus: Some(focus_border(ACCENT)),
            ..Default::default()
        },
        |ui| compose_contents(ui, demo),
    );
}

fn compose_contents(ui: &mut Ui, demo: &Demo) {
    demo.motion(ui);
    let spacing = Breakpoint::new(ui, "spacing");
    let compact = !spacing.fits(|full, given| full.preferred.height <= given.height);
    ui.scroll_column(
        Modifier::new()
            .gap(if compact { 16.dp() } else { 22.dp() })
            .fill_max_width()
            .fill_max_height()
            .background(BG)
            .padding(32.dp())
            .then(spacing.container(!compact)),
        |ui| {
            header(ui, demo);
            treatments(ui, demo);
            ui.row(Modifier::new().gap(32.dp()), |ui| {
                ui.column(Modifier::new().gap(14.dp()).weight(1.0), |ui| {
                    stage(ui, demo);
                    label(ui, CopyKey::Drag, 12.sp(), MUTED);
                });
                controls(ui, demo, compact);
            });
            ui.row(Modifier::new().gap(32.dp()), |ui| {
                palette(ui, demo);
                blending(ui, demo);
            });
            output(ui, demo);
        },
    );
}

fn header(ui: &mut Ui, demo: &Demo) {
    ui.row(Modifier::new().align_items(Align::Center), |ui| {
        ui.column(Modifier::new().gap(8.dp()), |ui| {
            ui.text(
                copy(CopyKey::Title),
                Modifier::new()
                    .font_family(DISPLAY)
                    .font_size(28.sp())
                    .font_weight(constant!(FontWeight::try_new(650.0)))
                    .semantics_with(|s| s.role = Some(Role::Heading))
                    .color(INK),
            );
            label(ui, CopyKey::Intro, 14.sp(), MUTED);
        });
        ui.space(Modifier::new());
        motion_toggle(ui, demo);
    });
}

fn motion_toggle(ui: &mut Ui, demo: &Demo) {
    let animate = demo.animate.get();
    let shown = if animate { demo.pause } else { demo.play };
    let state = demo.clone();
    ui.button(
        Modifier::new()
            .tag("motion")
            .semantics_with(move |s| {
                s.state.checked = Some(if animate {
                    Checked::True
                } else {
                    Checked::False
                });
            })
            .border(LINE, 1.dp(), 12.dp())
            .clip(12.dp())
            .padding_xy(18.dp(), 12.dp()),
        move || state.animate.set(!state.animate.get()),
        |ui| {
            ui.row(
                Modifier::new().gap(9.dp()).align_items(Align::Center),
                |ui| {
                    ui.icon(shown, IconOptions::default(), Modifier::new().color(ACCENT));
                    let action = if animate {
                        CopyKey::Pause
                    } else {
                        CopyKey::Animate
                    };
                    label(ui, action, 13.sp(), INK);
                },
            );
        },
    );
}

fn treatments(ui: &mut Ui, demo: &Demo) {
    ui.row(
        Modifier::new().gap(8.dp()).align_items(Align::Center),
        |ui| {
            label(ui, CopyKey::Materials, 12.sp(), MUTED);
            ui.spacer(12.dp(), 0.dp(), Modifier::new());
            for t in SurfaceTreatment::ALL {
                let state = demo.clone();
                let selected = demo.treatment.get() == t;
                ui.button(
                    Modifier::new()
                        .tag(key(t).to_string())
                        .focus_within(focus_border(if selected { BG } else { ACCENT }))
                        .selected(selected)
                        .clip(10.dp())
                        .background(if selected { ACCENT } else { SCALE.tone(0.23) })
                        .padding_xy(18.dp(), 11.dp()),
                    move || state.select(t),
                    |ui| {
                        label(ui, key(t), 13.sp(), if selected { BG } else { INK });
                    },
                );
            }
        },
    );
}

/// Design decisions of the demo: the stage is as tall as the window minus `KEPT_FOR_PAGE`, the height reserved for
/// the rest of the page, limited to the range from `SHORTEST_STAGE` to `TALLEST_STAGE`.
const KEPT_FOR_PAGE: Dp = Dp::new(570.0);
const SHORTEST_STAGE: Dp = Dp::new(250.0);
const TALLEST_STAGE: Dp = Dp::new(410.0);

fn stage(ui: &mut Ui, demo: &Demo) {
    let height = (ui.viewport().size.height - KEPT_FOR_PAGE).clamp(SHORTEST_STAGE, TALLEST_STAGE);
    let material = match demo.material() {
        Ok(material) => material,
        Err(error) => {
            ui.fail(error);
            return;
        }
    };
    let state = demo.clone();
    ui.canvas(
        Size::new(840.dp(), height),
        Modifier::new()
            .tag("stage")
            .fill_max_width()
            .height(height)
            .clip(20.dp())
            .on_pointer({
                let state = demo.clone();
                move |event| drag_panel(&state, event)
            }),
        move |d| {
            backdrop_shapes(d, state.phase.get());
            material_panel(d, &state, &material);
        },
    );
}

fn backdrop_shapes(d: &mut Drawing<'_>, phase: f64) {
    const PITCH: Dp = Dp::new(28.0);
    let size = d.size();
    let time = phase * std::f64::consts::TAU;
    let drift = Point::new(time.sin() * 50.dp(), time.cos() * size.height * 0.05);
    d.fill(
        Shape::rounded(Rect::new(0.dp(), 0.dp(), size.width, size.height), 20.dp()),
        Gradient::new(
            GradientKind::Linear {
                from: Point::new(0.dp(), 0.dp()),
                to: Point::new(size.width, size.height),
            },
            [
                ColorStop::new(0.0, Color::oklch(0.92, 0.04, 180.0)),
                ColorStop::new(0.5, Color::oklch(0.96, 0.026, 80.0)),
                ColorStop::new(1.0, Color::oklch(0.88, 0.05, 300.0)),
            ],
        ),
    );

    let grid = Color::oklch(0.35, 0.01, 180.0).with_alpha(0.13);
    let lines = |length: Dp| {
        (0_u16..)
            .map(|line| PITCH * f64::from(line))
            .take_while(move |at| *at < length)
    };
    for x in lines(size.width) {
        d.line(
            Point::new(x, 0.dp()),
            Point::new(x, size.height),
            0.65.dp(),
            grid,
        );
    }
    for y in lines(size.height) {
        d.line(
            Point::new(0.dp(), y),
            Point::new(size.width, y),
            0.65.dp(),
            grid,
        );
    }

    d.circle(
        Point::new(size.width * 0.25 + drift.x, size.height * 0.49 + drift.y),
        size.height * 0.33,
        Gradient::new(
            GradientKind::Linear {
                from: Point::new(65.dp(), 80.dp()),
                to: Point::new(300.dp(), 340.dp()),
            },
            [
                ColorStop::new(0.0, Color::oklch(0.8, 0.18, 55.0).mapped(Gamut::Srgb)),
                ColorStop::new(1.0, Color::oklch(0.63, 0.24, 25.0).mapped(Gamut::Srgb)),
            ],
        ),
    );
    d.circle(
        Point::new(size.width * 0.72 - drift.x, size.height * 0.56),
        size.height * 0.30,
        Gradient::new(
            GradientKind::Linear {
                from: Point::new(430.dp(), 90.dp()),
                to: Point::new(730.dp(), 360.dp()),
            },
            [
                ColorStop::new(0.0, Color::oklch(0.77, 0.17, 305.0).mapped(Gamut::Srgb)),
                ColorStop::new(1.0, Color::oklch(0.48, 0.23, 285.0).mapped(Gamut::Srgb)),
            ],
        ),
    );
    d.fill(
        Shape::rounded(
            Rect::new(
                size.width * 0.42,
                size.height * 0.10 - drift.y,
                110.dp(),
                size.height * 0.77,
            ),
            55.dp(),
        ),
        Color::oklch(0.85, 0.18, 145.0).mapped(Gamut::Srgb),
    );
}

fn material_panel(d: &mut Drawing<'_>, demo: &Demo, material: &Material) {
    let p = demo.panel_position(d.size());
    let shape = Shape::rounded(
        Rect::new(p.x, p.y, PANEL.width, PANEL.height),
        demo.radius.get(),
    );
    d.material(shape, material);

    let text = if matches!(
        demo.treatment.get(),
        SurfaceTreatment::BrushedMetal | SurfaceTreatment::Paper | SurfaceTreatment::Ceramic
    ) {
        Color::oklch(0.25, 0.02, 80.0)
    } else {
        Color::oklch(0.22, 0.025, 275.0)
    };
    d.text(
        copy(key(demo.treatment.get())),
        Rect::new(p.x + 30.dp(), p.y + 25.dp(), 300.dp(), 45.dp()),
        TextStyle::new(DISPLAY, 29.sp(), text).weight(constant!(FontWeight::try_new(650.0))),
    );
    d.text(
        format!("OKLCH  ·  {:.0}°", demo.hue.get()),
        Rect::new(p.x + 32.dp(), p.y + 73.dp(), 290.dp(), 24.dp()),
        TextStyle::new(BODY, 12.sp(), text.with_alpha(0.75)),
    );
    d.line(
        Point::new(p.x + 32.dp(), p.y + 117.dp()),
        Point::new(p.x + 323.dp(), p.y + 117.dp()),
        1.dp(),
        text.with_alpha(0.18),
    );

    for swatch in (0..5_u8).map(f64::from) {
        d.circle(
            Point::new(p.x + 47.dp() + 57.dp() * swatch, p.y + 160.dp()),
            13.dp(),
            Color::oklch(0.57 + swatch * 0.075, 0.08, demo.hue.get() + swatch * 15.0)
                .mapped(Gamut::Srgb),
        );
    }
}

fn drag_panel(demo: &Demo, event: PointerInput) {
    let pos = event.position;
    let panel = demo.panel_position(event.size);
    let held = matches!(event.phase, PointerPhase::Pressed | PointerPhase::Moved);
    if held && demo.dragging.get().is_none() {
        demo.dragging.set(
            Rect::new(panel.x, panel.y, PANEL.width, PANEL.height)
                .contains(pos)
                .then_some(Point::new(pos.x - panel.x, pos.y - panel.y)),
        );
    }
    if !held {
        demo.dragging.set(None);
    }
    if let Some(offset) = demo.dragging.get() {
        let dragged = Point::new(pos.x - offset.x, pos.y - offset.y);
        demo.position.set(inside(dragged, event.size));
    }
}
