// SPDX-License-Identifier: GPL-3.0-only
mod range;
mod timeline;

use crate::{
    CopyKey, copy,
    model::Demo,
    theme::{BACKGROUND, BLUE, LINE, MUTED, PALE, SURFACE, body, button, interaction, title},
};
use lavashell::prelude::*;
use lavashell_example_support::{Breakpoint, report, report_failure};
use range::slider;
use std::time::Duration;
use timeline::timeline;

fn control(ui: &mut Ui, key: CopyKey, modifier: Modifier, action: impl FnMut() + 'static) {
    ui.button(button().tag(copy(key)).then(modifier), action, |ui| {
        ui.text(
            copy(key),
            Modifier::new()
                .font_weight(FontWeight::SEMI_BOLD)
                .color(BLUE),
        );
    });
}

/// A design decision of the demo: the demo opens as a desktop window of this size. Its four panels fill the
/// window in two rows of two.
pub const WINDOW: Size = Size::new(Dp::new(1240.0), Dp::new(900.0));

/// The space around the content of the window and between its panels.
const PADDING: Dp = Dp::new(32.0);
const GAP: Dp = Dp::new(20.0);

pub fn compose(ui: &mut Ui, demo: &Demo) {
    if let Err(error) = ui.motion().bind(&demo.spring) {
        ui.fail(error);
    }
    if let Err(error) = ui.motion().bind(&demo.timeline) {
        ui.fail(error);
    }
    let preference_reduced = ui.local(&REDUCED_MOTION);
    let reduced = demo.reduced.get() || preference_reduced;
    let panels_per_row = 2.0;
    let width = (ui.viewport().size.width - PADDING * 2.0 - GAP) / panels_per_row;
    let heights = Breakpoint::new(ui, "heights");
    let compact = !heights.fits(|full, given| full.preferred.height <= given.height);
    ui.provide(&TEXT_STYLE, body(), |ui| {
        ui.provide(&INTERACTION_STYLE, interaction(), |ui| {
            ui.column(
                Modifier::new()
                    .gap(GAP)
                    .fill_max_width()
                    .fill_max_height()
                    .background(BACKGROUND)
                    .padding(PADDING)
                    .then(heights.container(!compact)),
                |ui| {
                    header(ui, demo, reduced, preference_reduced);
                    ui.row(Modifier::new().gap(GAP), |ui| {
                        panel(ui, CopyKey::AutoTitle, CopyKey::AutoHint, width, |ui| {
                            automatic(ui, demo, width - 48.dp(), compact);
                        });
                        panel(ui, CopyKey::SpringTitle, CopyKey::SpringHint, width, |ui| {
                            spring(ui, demo, width - 48.dp(), compact);
                        });
                    });
                    ui.row(Modifier::new().gap(GAP), |ui| {
                        panel(ui, CopyKey::MorphTitle, CopyKey::MorphHint, width, |ui| {
                            morph(ui, demo, width - 48.dp(), compact);
                        });
                        panel(
                            ui,
                            CopyKey::TimelineTitle,
                            CopyKey::TimelineHint,
                            width,
                            |ui| {
                                timeline(ui, demo, width - 48.dp(), compact);
                            },
                        );
                    });
                    status(ui, demo);
                },
            );
        });
    });
}

fn header(ui: &mut Ui, demo: &Demo, reduced: bool, preference_reduced: bool) {
    ui.row(
        Modifier::new()
            .gap(16.dp())
            .align_items(Align::Center)
            .fill_max_width(),
        |ui| {
            ui.column(Modifier::new().gap(6.dp()), |ui| {
                ui.text(
                    copy(CopyKey::Title),
                    Modifier::new()
                        .text_style(title(32.sp()))
                        .semantics_with(|s| s.role = Some(Role::Heading)),
                );
                ui.text(copy(CopyKey::Subtitle), Modifier::new().color(MUTED));
            });
            ui.space(Modifier::new());
            let (d, icons) = (demo.clone(), demo.icons);
            ui.toggle(
                demo.autoplay.get(),
                button().tag(copy(CopyKey::AutoPlay)),
                move |value| d.autoplay.set(value),
                move |ui, value| {
                    ui.row(
                        Modifier::new().gap(8.dp()).align_items(Align::Center),
                        |ui| {
                            ui.icon(
                                if value { icons.pause } else { icons.play },
                                IconOptions {
                                    size: Some(16.dp()),
                                    ..IconOptions::default()
                                },
                                Modifier::new().color(BLUE),
                            );
                            ui.text(copy(CopyKey::AutoPlay), Modifier::new().color(BLUE));
                        },
                    );
                },
            );
            let d = demo.clone();
            ui.toggle(
                reduced,
                Modifier::new()
                    .padding_xy(12.dp(), 10.dp())
                    .enabled(!preference_reduced)
                    .tag(copy(CopyKey::ReducedMotion)),
                move |value| report_failure("reduced motion", d.set_reduced(value)),
                move |ui, value| {
                    ui.row(
                        Modifier::new().gap(8.dp()).align_items(Align::Center),
                        |ui| {
                            ui.icon(
                                if value { icons.check } else { icons.minus },
                                IconOptions {
                                    size: Some(16.dp()),
                                    ..IconOptions::default()
                                },
                                Modifier::new().color(MUTED),
                            );
                            ui.text(copy(CopyKey::ReducedMotion), Modifier::new().color(MUTED));
                        },
                    );
                },
            );
        },
    );
}

fn status(ui: &mut Ui, demo: &Demo) {
    let d = demo.clone();
    ui.canvas(
        Size::new(ui.viewport().size.width - PADDING * 2.0, 30.dp()),
        Modifier::new(),
        move |drawing| {
            let frame = d.time.get();
            let refresh = frame.refresh.map_or_else(
                || copy(CopyKey::Variable).into(),
                |r| format!("{:.0} Hz", 1.0 / r.as_secs_f64()),
            );
            drawing.text(
                format!("{}  {:.2} ms", copy(CopyKey::FrameTime), d.render_ms.get()),
                Rect::new(0.dp(), 0.dp(), 240.dp(), 26.dp()),
                body().size(12.sp()).color(MUTED),
            );
            drawing.text(
                format!("{}  {refresh}", copy(CopyKey::Refresh)),
                Rect::new(280.dp(), 0.dp(), 400.dp(), 26.dp()),
                body().size(12.sp()).color(MUTED),
            );
            drawing.text(
                "Vulkan / Wayland",
                Rect::new(drawing.size().width - 130.dp(), 0.dp(), 130.dp(), 26.dp()),
                body().size(12.sp()).color(MUTED),
            );
        },
    );
}

fn panel(ui: &mut Ui, heading: CopyKey, hint: CopyKey, width: Dp, content: impl FnOnce(&mut Ui)) {
    ui.column(
        Modifier::new()
            .gap(9.dp())
            .width(width)
            .clip(18.dp())
            .background(SURFACE)
            .padding(24.dp()),
        |ui| {
            ui.text(
                copy(heading),
                Modifier::new()
                    .text_style(title(21.sp()))
                    .semantics_with(|s| s.role = Some(Role::Heading)),
            );
            ui.text(
                copy(hint),
                Modifier::new()
                    .color(MUTED)
                    .font_size(13.sp())
                    .max_lines(2)
                    .height(36.dp()),
            );
            content(ui);
        },
    );
}

fn grid(d: &mut Drawing<'_>) {
    const PITCH: Dp = Dp::new(24.0);
    let size = d.size();
    let lines = |length: Dp| {
        (0_u16..)
            .map(|line| PITCH * f64::from(line))
            .take_while(move |at| *at <= length)
    };
    for x in lines(size.width) {
        for y in lines(size.height) {
            d.circle(Point::new(x, y), 0.8.dp(), LINE);
        }
    }
}

fn automatic(ui: &mut Ui, demo: &Demo, width: Dp, compact: bool) {
    let height = if compact { 136.dp() } else { 176.dp() };
    let expanded = demo.expanded.get();
    let (card, radius) = if expanded {
        (
            Rect::new(26.dp(), 12.dp(), width - 52.dp(), height - 28.dp()),
            24.dp(),
        )
    } else {
        let size = Size::new(212.dp(), 96.dp());
        let at = Point::new(width - 242.dp(), (height - size.height) / 2.0);
        (Rect::new(at.x, at.y, size.width, size.height), 40.dp())
    };
    let spring = demo.spec(spring!(Duration::from_millis(680), 0.83));
    let size = match ui.animate("card-content", Size::new(card.width, card.height), spring) {
        Ok(size) => size,
        Err(error) => {
            ui.fail(error);
            return;
        }
    };
    ui.stack(Modifier::new().size(width, height), |ui| {
        ui.canvas(Size::new(width, height), Modifier::new(), grid);
        let d = demo.clone();
        ui.shared(
            "music-card",
            Modifier::new()
                .size(card.width, card.height)
                .offset(card.x, card.y)
                .clip(radius)
                .background(BLUE)
                .transition(spring)
                .on_click(move || d.expanded.update(|v| *v = !*v))
                .accessible_name("Undertow")
                .accessible_description(copy(CopyKey::AutoHint))
                .semantics_with(move |s| s.state.expanded = Some(expanded))
                .tag("Undertow"),
            |ui| {
                ui.canvas(
                    Size::new(card.width, card.height),
                    Modifier::new(),
                    move |drawing| {
                        paint_card(drawing, size.get());
                    },
                );
            },
        );
    });
    let d = demo.clone();
    control(
        ui,
        if expanded {
            CopyKey::Collapse
        } else {
            CopyKey::Expand
        },
        Modifier::new(),
        move || d.expanded.update(|v| *v = !*v),
    );
}

fn paint_card(drawing: &mut Drawing<'_>, live: Size) {
    let s = drawing.size();
    let aspect = (live.height / s.height) / (live.width / s.width);
    let card_height = s.height.get();
    drawing.transformed(
        Affine::scale(aspect, 1.0).translated(Point::new(s.width * 0.82 * (1.0 - aspect), 0.dp())),
        |drawing| {
            drawing.circle(
                Point::new(s.width * 0.82, s.height * 0.3),
                s.height * 0.68,
                Color::rgba(0x0076_92fa, 100),
            );
            drawing.circle(
                Point::new(s.width * 0.82, s.height * 0.3),
                s.height * 0.43,
                Color::rgba(0x00b6_c8ff, 90),
            );
            drawing.circle(
                Point::new(s.width * 0.82, s.height * 0.3),
                s.height * 0.2,
                Color::rgba(0x00f6_d490, 240),
            );
        },
    );
    drawing.transformed(
        Affine::scale(aspect, 1.0).translated(Point::new(s.width * 0.1 * (1.0 - aspect), 0.dp())),
        |drawing| {
            drawing.text(
                "Undertow",
                Rect::new(s.width * 0.1, s.height * 0.5, s.width * 0.65, 36.dp()),
                title(Sp::new(card_height * 0.16)).color(Color::WHITE),
            );
            drawing.text(
                "Signal",
                Rect::new(s.width * 0.1, s.height * 0.76, s.width * 0.7, 22.dp()),
                body()
                    .size(Sp::new(card_height * 0.075))
                    .color(Color::rgba(0x00ff_ffff, 200)),
            );
        },
    );
}

fn spring(ui: &mut Ui, demo: &Demo, width: Dp, compact: bool) {
    let height = if compact { 136.dp() } else { 176.dp() };
    let d = demo.clone();
    ui.canvas(Size::new(width, height), Modifier::new(), move |drawing| {
        grid(drawing);
        let target = d.target.get() * (width - 56.dp()) + 28.dp();
        let position = d.spring.get() * (width - 56.dp()) + 28.dp();
        drawing.fill(
            Shape::rounded(
                Rect::new(28.dp(), height * 0.52 - 1.dp(), width - 56.dp(), 2.dp()),
                1.dp(),
            ),
            LINE,
        );
        drawing.circle(Point::new(target, height * 0.52), 27.dp(), PALE);
        drawing.circle(Point::new(target, height * 0.52), 2.5.dp(), BLUE);
        drawing.circle(Point::new(position, height * 0.52), 18.dp(), BLUE);
        drawing.circle(
            Point::new(position - 5.dp(), height * 0.52 - 5.dp()),
            4.dp(),
            Color::rgba(0x00ff_ffff, 160),
        );
        drawing.text(
            format!("{:+.2}", d.spring.velocity().0[0]),
            Rect::new(26.dp(), height - 27.dp(), 200.dp(), 22.dp()),
            body().size(12.sp()).color(MUTED),
        );
    });
    let d = demo.clone();
    let target = demo.target.clone();
    slider(
        ui,
        move || target.get(),
        width,
        Modifier::new()
            .tag("spring-target")
            .accessible_name(copy(CopyKey::SpringTitle))
            .accessible_description(copy(CopyKey::SpringHint)),
        move |value| report_failure("spring target", d.move_target(value)),
    );
}

fn morph(ui: &mut Ui, demo: &Demo, width: Dp, compact: bool) {
    let height = if compact { 86.dp() } else { 118.dp() };
    let radius = height * 0.46;
    // One device pixel over the diameter, as a fraction of the radius, places every edge within half a device
    // pixel of the outlines. That is the precision of rounded corners.
    let shape = match demo.morph(ui.viewport().pixel() / (radius * 2.0)) {
        Ok(shape) => shape,
        Err(error) => {
            report("morph", error);
            return;
        }
    };
    if let Err(error) = ui.motion().bind(&shape) {
        ui.fail(error);
    }
    ui.canvas(Size::new(width, height), Modifier::new(), move |drawing| {
        let contour = shape.get();
        let center = Point::new(width / 2.0, height / 2.0);
        let points: Vec<_> = (contour.points().iter())
            .map(|p| center.offset(radius * p.x, radius * p.y))
            .collect();
        drawing.circle(center, height * 0.47, PALE);
        drawing.path(&points, true, BLUE, None);
        drawing.circle(
            center.offset(-14.dp(), -14.dp()),
            6.dp(),
            Color::rgba(0x00ff_ffff, 180),
        );
    });
    let d = demo.clone();
    control(ui, CopyKey::Morph, Modifier::new(), move || {
        report_failure("morph", d.change_shape());
    });
}
