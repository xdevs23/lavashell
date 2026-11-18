// SPDX-License-Identifier: GPL-3.0-only
use crate::{
    CopyKey, copy,
    model::{BackMotion, Demo, Page},
    tag::Tag,
    theme::{PALETTE, Palette, body, title},
};
use lavashell::prelude::*;
use lavashell_example_support::report_failure;
use std::time::Duration;

const STAGE_HEIGHT: Dp = Dp::new(320.0);
const PAGE_PADDING: Dp = Dp::new(16.0);
const LIST_PARALLAX: f64 = 0.15;
const PAGE_SHADOW: Shadow = constant!(Shadow::new(
    Color::rgba(0x0000_0000, 56),
    Dp::new(18.0),
    Point::new(Dp::new(-4.0), Dp::ZERO),
));

#[derive(Clone, Copy, Hash)]
enum Setting {
    Network,
    PairedDevices,
    Display,
    Sound,
    Notifications,
}

impl Setting {
    fn name(self) -> CopyKey {
        match self {
            Self::Network => CopyKey::Network,
            Self::PairedDevices => CopyKey::PairedDevices,
            Self::Display => CopyKey::Display,
            Self::Sound => CopyKey::Sound,
            Self::Notifications => CopyKey::Notifications,
        }
    }

    fn detail(self) -> CopyKey {
        match self {
            Self::Network => CopyKey::NetworkDetail,
            Self::PairedDevices => CopyKey::PairedDevicesDetail,
            Self::Display => CopyKey::DisplayDetail,
            Self::Sound => CopyKey::SoundDetail,
            Self::Notifications => CopyKey::NotificationsDetail,
        }
    }

    fn icon_color(self) -> Color {
        match self {
            Self::Network => Color::rgb(0x003b_82f6),
            Self::PairedDevices => Color::rgb(0x0063_66f1),
            Self::Display => Color::rgb(0x00f5_9e0b),
            Self::Sound => Color::rgb(0x00ec_4899),
            Self::Notifications => Color::rgb(0x00ef_4444),
        }
    }
}

pub(super) fn settings(ui: &mut Ui, demo: &Demo, width: Dp, reduced: bool) {
    let colors = ui.local(&PALETTE);
    let progress = demo.back.progress.clone();
    let open = demo.back.shows_display();
    let stage = Modifier::new()
        .size(width, STAGE_HEIGHT)
        .clip(16.dp())
        .background(colors.background)
        .tag(Tag::SettingsStage);
    let motion = if reduced {
        BackMotion::Jumps
    } else {
        let landing = spring!(Duration::from_millis(460), 0.9).into();
        BackMotion::FollowsFinger { landing }
    };
    let stage = if open {
        let back = demo.back.clone();
        stage.on_drag(Axis::Horizontal, move |drag| {
            report_failure("back gesture", back.drag(drag, motion));
        })
    } else {
        stage
    };
    ui.stack(stage, |ui| {
        let list = slid(&progress, move |at| width * LIST_PARALLAX * (at - 1.0));
        let list = ui.view(Page::Settings, list.size(width, STAGE_HEIGHT), |ui| {
            settings_list(ui, demo, motion, open, &colors);
        });
        if open {
            let page = slid(&progress, move |at| width * at)
                .size(width, STAGE_HEIGHT)
                .background(colors.surface)
                .shadow(PAGE_SHADOW, Dp::ZERO);
            let page = ui.view(Page::Display, page, |ui| {
                display_page(ui, demo, motion, width, &colors);
            });
            ui.crossing(page, list, &progress);
        }
    });
}

fn slid(progress: &Animation<f64>, x_at: impl Fn(f64) -> Dp + 'static) -> Modifier {
    Modifier::new().offset(Deferred::<f64>::from(progress).map(x_at), Dp::ZERO)
}

fn settings_list(ui: &mut Ui, demo: &Demo, motion: BackMotion, open: bool, colors: &Palette) {
    let back = demo.back.clone();
    let opens_display = Modifier::new()
        .tag(Tag::DisplayRow)
        .enabled(!open)
        .on_activate(move |_| {
            report_failure("open display", back.go_to(Page::Display, motion));
        });
    ui.column(
        Modifier::new()
            .fill_max_width()
            .gap(4.dp())
            .padding(12.dp()),
        |ui| {
            ui.text(
                copy(CopyKey::Settings),
                Modifier::new()
                    .text_style(title(colors.ink, 20.sp()))
                    .padding_xy(4.dp(), 4.dp()),
            );
            row(ui, Setting::Network, Modifier::new(), colors);
            row(ui, Setting::PairedDevices, Modifier::new(), colors);
            row(ui, Setting::Display, opens_display, colors);
            row(ui, Setting::Sound, Modifier::new(), colors);
            row(ui, Setting::Notifications, Modifier::new(), colors);
        },
    );
}

fn row(ui: &mut Ui, setting: Setting, modifier: Modifier, colors: &Palette) {
    let look = Modifier::new()
        .fill_max_width()
        .gap(10.dp())
        .align_items(Align::Center)
        .clip(10.dp())
        .background(colors.surface)
        .padding_xy(8.dp(), 6.dp());
    ui.row(look.then(modifier), |ui| {
        let icon = Modifier::new()
            .clip(6.dp())
            .background(setting.icon_color());
        ui.spacer(24.dp(), 24.dp(), icon);
        ui.column(Modifier::new(), |ui| {
            ui.shared(setting, Modifier::new(), |ui| {
                ui.text(copy(setting.name()), Modifier::new());
            });
            detail(ui, setting.detail(), colors);
        });
    });
}

fn display_page(ui: &mut Ui, demo: &Demo, motion: BackMotion, width: Dp, colors: &Palette) {
    let back = demo.back.clone();
    ui.column(
        Modifier::new()
            .fill_max_width()
            .gap(12.dp())
            .padding(PAGE_PADDING),
        |ui| {
            super::control(ui, CopyKey::Back, Modifier::new(), move || {
                report_failure("back", back.go_to(Page::Settings, motion));
            });
            ui.shared(Setting::Display, Modifier::new(), |ui| {
                let header = title(colors.ink, 22.sp());
                ui.text(
                    copy(Setting::Display.name()),
                    Modifier::new().text_style(header),
                );
            });
            detail(ui, CopyKey::Brightness, colors);
            brightness(ui, width - PAGE_PADDING * 2.0, colors);
            ui.column(Modifier::new(), |ui| {
                ui.text(copy(CopyKey::NightLight), Modifier::new());
                detail(ui, CopyKey::NightLightDetail, colors);
            });
            ui.column(Modifier::new(), |ui| {
                ui.text(copy(CopyKey::ScreenTimeout), Modifier::new());
                detail(ui, CopyKey::ScreenTimeoutDetail, colors);
            });
        },
    );
}

fn detail(ui: &mut Ui, key: CopyKey, colors: &Palette) {
    let style = body(colors.muted).size(11.sp());
    ui.text(copy(key), Modifier::new().text_style(style));
}

fn brightness(ui: &mut Ui, width: Dp, colors: &Palette) {
    let colors = *colors;
    let level = width * 0.64;
    ui.canvas(Size::new(width, 16.dp()), Modifier::new(), move |d| {
        let track = |length| Shape::rounded(Rect::new(0.dp(), 6.dp(), length, 4.dp()), 2.dp());
        d.fill(track(width), colors.pale);
        d.fill(track(level), colors.amber);
        d.circle(Point::new(level, 8.dp()), 7.dp(), colors.surface);
        d.circle(Point::new(level, 8.dp()), 5.dp(), colors.amber);
    });
}
