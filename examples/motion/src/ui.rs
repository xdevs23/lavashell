// SPDX-License-Identifier: GPL-3.0-only
mod range;
mod timeline;

use crate::{
    CopyKey, copy,
    model::Demo,
    tag::Tag,
    theme::{PALETTE, Palette, body, button, interaction, title},
};
use lavashell::prelude::*;
use lavashell_example_support::{
    Breakpoint, Layout, ThemeSwitchColors, UNDERTOW, report, report_failure,
};
use range::slider;
use std::time::Duration;
use timeline::timeline;

fn control(ui: &mut Ui, key: CopyKey, modifier: Modifier, action: impl FnMut() + 'static) {
    let colors = ui.local(&PALETTE);
    ui.button(
        button(&colors).tag(Tag::Control(key)).then(modifier),
        action,
        |ui| {
            ui.text(
                copy(key),
                Modifier::new()
                    .font_weight(FontWeight::SEMI_BOLD)
                    .color(colors.blue),
            );
        },
    );
}

pub const WINDOW: Size = Size::new(Dp::new(1240.0), Dp::new(900.0));

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
    let layout = heights.layout(|full, given| full.preferred.height <= given.height);
    demo.theme.provide(ui, &PALETTE, Palette::of, |ui| {
        let colors = ui.local(&PALETTE);
        ui.provide(&TEXT_STYLE, body(colors.ink), |ui| {
            ui.provide(&INTERACTION_STYLE, interaction(&colors), |ui| {
                ui.column(
                    Modifier::new()
                        .gap(GAP)
                        .fill_max_width()
                        .fill_max_height()
                        .background(colors.background)
                        .padding(PADDING)
                        .then(layout.container(&heights)),
                    |ui| {
                        header(ui, demo, reduced, preference_reduced);
                        ui.row(Modifier::new().gap(GAP), |ui| {
                            panel(ui, CopyKey::AutoTitle, width, |ui| {
                                automatic(ui, demo, width - 48.dp(), layout);
                            });
                            panel(ui, CopyKey::SpringTitle, width, |ui| {
                                spring(ui, demo, width - 48.dp(), layout);
                            });
                        });
                        ui.row(Modifier::new().gap(GAP), |ui| {
                            panel(ui, CopyKey::MorphTitle, width, |ui| {
                                morph(ui, demo, width - 48.dp(), layout);
                            });
                            panel(ui, CopyKey::TimelineTitle, width, |ui| {
                                timeline(ui, demo, width - 48.dp(), layout);
                            });
                        });
                        status(ui, demo);
                    },
                );
            });
        });
    });
}

fn header(ui: &mut Ui, demo: &Demo, reduced: bool, preference_reduced: bool) {
    let colors = ui.local(&PALETTE);
    ui.row(
        Modifier::new()
            .gap(16.dp())
            .align_items(Align::Center)
            .fill_max_width(),
        |ui| {
            ui.text(
                copy(CopyKey::Title),
                Modifier::new()
                    .text_style(title(colors.ink, 32.sp()))
                    .semantics_with(|s| s.role = Some(Role::Heading)),
            );
            ui.space(Modifier::new());
            let (d, icons) = (demo.clone(), demo.icons);
            ui.toggle(
                demo.autoplay.get(),
                button(&colors).tag(Tag::Control(CopyKey::AutoPlay)),
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
                                Modifier::new().color(colors.blue),
                            );
                            ui.text(copy(CopyKey::AutoPlay), Modifier::new().color(colors.blue));
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
                    .tag(Tag::Control(CopyKey::ReducedMotion)),
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
                                Modifier::new().color(colors.muted),
                            );
                            ui.text(
                                copy(CopyKey::ReducedMotion),
                                Modifier::new().color(colors.muted),
                            );
                        },
                    );
                },
            );
            demo.theme.switch(
                ui,
                ThemeSwitchColors {
                    ink: colors.muted,
                    accent: colors.blue,
                },
            );
        },
    );
}

fn status(ui: &mut Ui, demo: &Demo) {
    let colors = ui.local(&PALETTE);
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
                format!("{}  {:.2?}", copy(CopyKey::FrameTime), d.render_cost.get()),
                Rect::new(0.dp(), 0.dp(), 240.dp(), 26.dp()),
                body(colors.muted).size(12.sp()),
            );
            drawing.text(
                format!("{}  {refresh}", copy(CopyKey::Refresh)),
                Rect::new(280.dp(), 0.dp(), 400.dp(), 26.dp()),
                body(colors.muted).size(12.sp()),
            );
        },
    );
}

fn panel(ui: &mut Ui, heading: CopyKey, width: Dp, content: impl FnOnce(&mut Ui)) {
    let colors = ui.local(&PALETTE);
    ui.column(
        Modifier::new()
            .gap(9.dp())
            .width(width)
            .clip(18.dp())
            .background(colors.surface)
            .padding(24.dp()),
        |ui| {
            ui.text(
                copy(heading),
                Modifier::new()
                    .text_style(title(colors.ink, 21.sp()))
                    .semantics_with(|s| s.role = Some(Role::Heading)),
            );
            content(ui);
        },
    );
}

fn grid(d: &mut Drawing<'_>, line: Color) {
    const PITCH: Dp = Dp::new(24.0);
    let size = d.size();
    let lines = |length: Dp| {
        (0_u16..)
            .map(|line| PITCH * f64::from(line))
            .take_while(move |at| *at <= length)
    };
    for x in lines(size.width) {
        for y in lines(size.height) {
            d.circle(Point::new(x, y), 0.8.dp(), line);
        }
    }
}

fn automatic(ui: &mut Ui, demo: &Demo, width: Dp, layout: Layout) {
    let colors = ui.local(&PALETTE);
    let height = layout.choose(136.dp(), 176.dp());
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
        ui.canvas(Size::new(width, height), Modifier::new(), move |d| {
            grid(d, colors.line);
        });
        let d = demo.clone();
        ui.shared(
            "music-card",
            Modifier::new()
                .size(card.width, card.height)
                .offset(card.x, card.y)
                .clip(radius)
                .background(colors.blue)
                .transition(spring)
                .on_click(move || d.expanded.update(|v| *v = !*v))
                .accessible_name(UNDERTOW.title)
                .semantics_with(move |s| s.state.expanded = Some(expanded))
                .tag(Tag::Card),
            |ui| {
                ui.canvas(
                    Size::new(card.width, card.height),
                    Modifier::new(),
                    move |drawing| {
                        paint_card(drawing, size.get(), &colors);
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

fn paint_card(drawing: &mut Drawing<'_>, live: Size, colors: &Palette) {
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
                UNDERTOW.title,
                Rect::new(s.width * 0.1, s.height * 0.5, s.width * 0.65, 36.dp()),
                title(colors.on_blue, Sp::new(card_height * 0.16)),
            );
            drawing.text(
                UNDERTOW.artist,
                Rect::new(s.width * 0.1, s.height * 0.76, s.width * 0.7, 22.dp()),
                body(colors.on_blue.with_alpha(0.8)).size(Sp::new(card_height * 0.075)),
            );
        },
    );
}

fn spring(ui: &mut Ui, demo: &Demo, width: Dp, layout: Layout) {
    let colors = ui.local(&PALETTE);
    let height = layout.choose(136.dp(), 176.dp());
    let d = demo.clone();
    ui.canvas(Size::new(width, height), Modifier::new(), move |drawing| {
        grid(drawing, colors.line);
        let target = d.target.get() * (width - 56.dp()) + 28.dp();
        let position = d.spring.get() * (width - 56.dp()) + 28.dp();
        drawing.fill(
            Shape::rounded(
                Rect::new(28.dp(), height * 0.52 - 1.dp(), width - 56.dp(), 2.dp()),
                1.dp(),
            ),
            colors.line,
        );
        drawing.circle(Point::new(target, height * 0.52), 27.dp(), colors.pale);
        drawing.circle(Point::new(target, height * 0.52), 2.5.dp(), colors.blue);
        drawing.circle(Point::new(position, height * 0.52), 18.dp(), colors.blue);
        drawing.circle(
            Point::new(position - 5.dp(), height * 0.52 - 5.dp()),
            4.dp(),
            Color::rgba(0x00ff_ffff, 160),
        );
        drawing.text(
            format!("{:+.2}", d.spring.velocity().0[0]),
            Rect::new(26.dp(), height - 27.dp(), 200.dp(), 22.dp()),
            body(colors.muted).size(12.sp()),
        );
    });
    let d = demo.clone();
    let target = demo.target.clone();
    slider(
        ui,
        move || target.get(),
        width,
        Modifier::new()
            .tag(Tag::SpringTarget)
            .accessible_name(copy(CopyKey::SpringTitle)),
        move |value| report_failure("spring target", d.move_target(value)),
    );
}

fn morph(ui: &mut Ui, demo: &Demo, width: Dp, layout: Layout) {
    let colors = ui.local(&PALETTE);
    let height = layout.choose(86.dp(), 118.dp());
    let radius = height * 0.46;
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
        drawing.circle(center, height * 0.47, colors.pale);
        drawing.path(&points, true, colors.blue, None);
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
