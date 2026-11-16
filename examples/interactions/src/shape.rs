// SPDX-License-Identifier: GPL-3.0-only
use crate::{
    Error,
    copy::{CopyKey, copy},
    model::Demo,
    palette::PALETTE,
    samples::{SAMPLES_DIRECTORY, SampleFiles},
    tag::Tag,
    ui,
};
use lavashell::core::numbers::{count_f64, whole_count};
use lavashell::core::surface::SurfaceId;
use lavashell::prelude::*;
use lavashell_example_support::{report, report_failure};
use rustix::process::{Pid, PidfdFlags, pidfd_open};
use std::cell::RefCell;
use std::f64::consts::{PI, TAU};
use std::ffi::OsString;
use std::os::fd::OwnedFd;
use std::process::{Child, Command};
use std::rc::Rc;

const WINDOW: Size = Size::new(Dp::new(720.0), Dp::new(690.0));
const MIN_WINDOW: Size = Size::new(Dp::new(600.0), Dp::new(570.0));
const HOLE_RADIUS: Dp = Dp::new(28.0);
const HOLE_TOLERANCE: Dp = Dp::new(0.5);

pub struct ChildWindow {
    process: Option<Child>,
    open: State<bool>,
    samples: Rc<SampleFiles>,
    arguments: Vec<OsString>,
}

impl ChildWindow {
    pub fn new(open: State<bool>, samples: Rc<SampleFiles>, arguments: Vec<OsString>) -> Self {
        Self {
            process: None,
            open,
            samples,
            arguments,
        }
    }

    pub fn open(&mut self) -> Result<Option<OwnedFd>, Error> {
        self.reap();
        if self.process.is_some() {
            return Ok(None);
        }
        let mut command = Command::new(std::env::current_exe().map_err(Error::CurrentExecutable)?);
        let _chained_command = (command.arg("--shaped").args(&self.arguments))
            .env(SAMPLES_DIRECTORY, self.samples.directory());
        let child = command.spawn().map_err(Error::StartChild)?;
        let exited = match pidfd_open(Pid::from_child(&child), PidfdFlags::empty()) {
            Ok(exited) => exited,
            Err(error) => {
                stop(child);
                return Err(Error::WatchChild(error));
            }
        };
        self.process = Some(child);
        self.open.set_if_changed(true);
        Ok(Some(exited))
    }

    pub fn reap(&mut self) {
        let Some(child) = &mut self.process else {
            return;
        };
        match child.try_wait() {
            Ok(Some(_)) => {
                self.process = None;
                self.open.set_if_changed(false);
            }
            Ok(None) => {}
            Err(error) => report("shaped child status", error),
        }
    }

    pub fn close(&mut self) {
        if let Some(mut child) = self.process.take()
            && !matches!(child.try_wait(), Ok(Some(_)))
        {
            stop(child);
        }
        self.open.set_if_changed(false);
    }
}

fn stop(mut child: Child) {
    if let Err(error) = child.kill() {
        report("shaped child termination", error);
    }
    if let Err(error) = child.wait() {
        report("shaped child reap", error);
    }
}

impl Drop for ChildWindow {
    fn drop(&mut self) {
        self.close();
    }
}

pub struct Hole {
    angles: Vec<f64>,
}

impl Hole {
    pub fn new() -> Result<Self, Error> {
        let corners = (PI / (1.0 - HOLE_TOLERANCE / HOLE_RADIUS).acos()).ceil();
        let corners = whole_count(corners).map_err(Error::HoleCorners)?;
        let step = TAU / count_f64(corners).map_err(Error::HoleCorners)?;
        let angles = (0..corners)
            .map(|corner| count_f64(corner).map(|corner| corner * step))
            .collect::<Result<Vec<_>, _>>()
            .map_err(Error::HoleCorners)?;
        Ok(Self { angles })
    }

    fn corners(&self, center: Point) -> Vec<Point> {
        self.angles
            .iter()
            .map(|angle| {
                Point::new(
                    center.x + HOLE_RADIUS * angle.cos(),
                    center.y + HOLE_RADIUS * angle.sin(),
                )
            })
            .collect()
    }
}

fn outer(size: Size) -> Vec<Point> {
    vec![
        Point::new(18.dp(), 18.dp()),
        Point::new(size.width - 18.dp(), 18.dp()),
        Point::new(size.width - 18.dp(), size.height - 18.dp()),
        Point::new(size.width * 0.28, size.height - 18.dp()),
        Point::new(size.width * 0.28, size.height * 0.76),
        Point::new(18.dp(), size.height * 0.76),
    ]
}

fn outline(size: Size, hole: &Hole) -> Vec<Vec<Point>> {
    vec![outer(size), hole.corners(hole_center(size))]
}

fn hole_center(size: Size) -> Point {
    Point::new(size.width - 68.dp(), size.height * 0.40)
}

pub fn header(point: Point, size: Size) -> bool {
    Rect::new(
        24.dp(),
        24.dp(),
        (size.width - 48.dp()).max(Dp::ZERO),
        48.dp(),
    )
    .contains(point)
}

pub fn window(hole: Hole) -> Window {
    Window::new(copy(CopyKey::Title), "lavashell.interactions.shaped")
        .size(WINDOW.width, WINDOW.height)
        .min_size(MIN_WINDOW.width, MIN_WINDOW.height)
        .shape(WindowShape::new(move |size| outline(size, &hole)))
        .drag_region(header)
}

pub fn compose(ui: &mut Ui, demo: &Demo, actions: &ViewActions) {
    let colors = ui.local(&PALETTE);
    ui::section(ui, "04", CopyKey::Shapes, |ui| {
        ui.row(
            Modifier::new().gap(20.dp()).align_items(Align::Center),
            |ui| {
                ui.canvas(Size::new(180.dp(), 120.dp()), Modifier::new(), move |d| {
                    d.fill(
                        Shape::rounded(Rect::new(0.dp(), 0.dp(), 180.dp(), 120.dp()), 12.dp()),
                        colors.background,
                    );
                    let thumbnail = Rect::new(8.dp(), 6.dp(), 158.dp(), 100.dp());
                    let shrink = |point: Point| {
                        Point::new(
                            thumbnail.x + point.x * (thumbnail.width / WINDOW.width),
                            thumbnail.y + point.y * (thumbnail.height / WINDOW.height),
                        )
                    };
                    let points = outer(WINDOW).into_iter().map(shrink).collect::<Vec<_>>();
                    d.path(&points, true, colors.card, None);
                    d.path(&points, true, colors.accent, Some(1.5.dp()));
                    d.circle(shrink(hole_center(WINDOW)), 5.dp(), colors.background);
                    d.fill(
                        Shape::rounded(Rect::new(21.dp(), 17.dp(), 130.dp(), 9.dp()), 3.dp()),
                        colors.line,
                    );
                    d.fill(
                        Shape::rounded(Rect::new(21.dp(), 35.dp(), 103.dp(), 5.dp()), 2.dp()),
                        colors.line,
                    );
                    d.fill(
                        Shape::rounded(Rect::new(21.dp(), 47.dp(), 89.dp(), 5.dp()), 2.dp()),
                        colors.line,
                    );
                });
                ui.column(Modifier::new().gap(20.dp()).weight(1.0), |ui| {
                    ui.row(Modifier::new().gap(8.dp()), |ui| {
                        let (opener, child, actions) =
                            (demo.clone(), demo.child.clone(), actions.clone());
                        ui.button(
                            Modifier::new()
                                .tag(Tag::Control(CopyKey::ShapeOpen))
                                .accessible_name(copy(CopyKey::ShapeOpen))
                                .enabled(!demo.child_open.get())
                                .border(colors.line, 1.dp(), 9.dp())
                                .padding(11.dp()),
                            move || {
                                let Some(window) = opener.window() else {
                                    return;
                                };
                                let outcome = child.borrow_mut().open();
                                match outcome {
                                    Ok(Some(exited)) => {
                                        watch_exit(&actions, window, exited, child.clone());
                                    }
                                    Ok(None) => {}
                                    Err(error) => report("shaped child", error),
                                }
                            },
                            |ui| ui::label(ui, CopyKey::ShapeOpen, 13.sp(), colors.ink),
                        );
                        let child = demo.child.clone();
                        ui.button(
                            Modifier::new()
                                .tag(Tag::Control(CopyKey::ShapeClose))
                                .accessible_name(copy(CopyKey::ShapeClose))
                                .enabled(demo.child_open.get())
                                .border(colors.line, 1.dp(), 9.dp())
                                .padding(11.dp()),
                            move || child.borrow_mut().close(),
                            |ui| ui::label(ui, CopyKey::ShapeClose, 13.sp(), colors.ink),
                        );
                    });
                });
            },
        );
    });
}

fn watch_exit(
    actions: &ViewActions,
    window: SurfaceId,
    exited: OwnedFd,
    child: Rc<RefCell<ChildWindow>>,
) {
    let watched =
        actions.dispatch_when_readable(exited, window, move |_| child.borrow_mut().reap());
    report_failure("shaped child", watched);
}

pub fn child_compose(ui: &mut Ui, demo: &Demo, clipboard: &Clipboard, actions: &ViewActions) {
    ui::themed(ui, demo, |ui| {
        let colors = ui.local(&PALETTE);
        ui.scroll_column(
            Modifier::new()
                .focusable(false)
                .gap(16.dp())
                .fill_max_width()
                .fill_max_height()
                .background(colors.background)
                .padding(36.dp()),
            |ui| {
                ui.row(Modifier::new().align_items(Align::Center), |ui| {
                    ui.text(
                        copy(CopyKey::Title),
                        Modifier::new()
                            .font_family(lavashell_example_support::DISPLAY)
                            .font_size(27.sp())
                            .font_weight(constant!(FontWeight::try_new(650.0)))
                            .color(colors.ink)
                            .semantics_with(|s| s.role = Some(Role::Heading)),
                    );
                    ui.space(Modifier::new());
                    ui::theme_switch(ui, demo);
                });
                let quit = demo.quit.clone();
                ui::button(ui, CopyKey::ShapeClose, Modifier::new(), move || {
                    quit.set(true);
                });
                crate::text::compose(ui, demo, clipboard, actions);
                crate::transfers::compose(ui, demo, clipboard);
            },
        );
    });
}
