// SPDX-License-Identifier: GPL-3.0-only
use super::control;
use super::range::slider;
use crate::{
    CopyKey, copy,
    model::{Demo, TIMELINE},
    tag::Tag,
    theme::{PALETTE, Palette, body},
};
use lavashell::prelude::*;
use lavashell_example_support::{Layout, report_failure};

pub(super) fn timeline(ui: &mut Ui, demo: &Demo, width: Dp, layout: Layout) {
    let height = layout.choose(86.dp(), 118.dp());
    let colors = ui.local(&PALETTE);
    let d = demo.clone();
    ui.canvas(Size::new(width, height), Modifier::new(), move |drawing| {
        paint_timeline(drawing, d.timeline.get(), &colors);
    });
    ui.row(
        Modifier::new().gap(10.dp()).align_items(Align::Center),
        |ui| {
            let d = demo.clone();
            let finished = demo.timeline.is_finished();
            control(
                ui,
                if demo.paused.get() {
                    CopyKey::Play
                } else {
                    CopyKey::Pause
                },
                Modifier::new()
                    .enabled(!finished)
                    .opacity(if finished { 0.4 } else { 1.0 }),
                move || d.toggle_pause(),
            );
            let d = demo.clone();
            control(ui, CopyKey::Replay, Modifier::new(), move || {
                report_failure("timeline replay", d.restart());
            });
            let d = demo.clone();
            let timeline = demo.timeline.clone();
            let length = demo.timeline_length();
            slider(
                ui,
                move || {
                    let progress = timeline.get();
                    if timeline.is_finished() {
                        return progress;
                    }
                    let lengths = timeline.elapsed().as_secs_f64() / length.as_secs_f64();
                    let back_and_forth = lengths % 2.0;
                    if back_and_forth <= 1.0 {
                        back_and_forth
                    } else {
                        2.0 - back_and_forth
                    }
                },
                (width - 196.dp()).max(100.dp()),
                Modifier::new()
                    .tag(Tag::TimelineSeek)
                    .accessible_name(copy(CopyKey::TimelineTitle))
                    .enabled(!demo.timeline.is_finished()),
                move |value| d.timeline.seek(length.mul_f64(value)),
            );
        },
    );
}

fn paint_timeline(drawing: &mut Drawing<'_>, progress: f64, colors: &Palette) {
    let size = drawing.size();
    let height = size.height;
    let x = 25.dp() + progress * (size.width - 50.dp());
    drawing.fill(
        Shape::rounded(
            Rect::new(
                25.dp(),
                height * 0.48 - 1.dp(),
                size.width - 50.dp(),
                2.dp(),
            ),
            1.dp(),
        ),
        colors.line,
    );
    for (_, t) in TIMELINE {
        drawing.circle(
            Point::new(25.dp() + t * (size.width - 50.dp()), height * 0.48),
            5.dp(),
            colors.pale,
        );
    }
    drawing.fill(
        Shape::rounded(
            Rect::new(x - 16.dp(), height * 0.48 - 16.dp(), 32.dp(), 32.dp()),
            11.dp(),
        ),
        colors.amber,
    );
    drawing.text(
        format!("{progress:.3}"),
        Rect::new(25.dp(), height - 24.dp(), 120.dp(), 24.dp()),
        body(colors.muted).size(12.sp()),
    );
}
