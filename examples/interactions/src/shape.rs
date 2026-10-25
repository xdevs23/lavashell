// SPDX-License-Identifier: GPL-3.0-only
use crate::{
    Error,
    copy::{CopyKey, copy},
    model::Demo,
    samples::SampleFiles,
    ui,
};
use lavashell::core::numbers::{count_f64, whole_count};
use lavashell::prelude::*;
use lavashell_example_support::report;
use rustix::process::{Pid, PidfdFlags, pidfd_open};
use std::f64::consts::{PI, TAU};
use std::ffi::OsString;
use std::os::fd::OwnedFd;
use std::process::{Child, Command};
use std::rc::Rc;

/// A design decision of the demo: the shaped window opens at this size. The thumbnail in the main window draws the
/// outline of the shaped window at this size, scaled down.
const WINDOW: Size = Size::new(Dp::new(720.0), Dp::new(690.0));
/// A design decision of the demo: the smallest size of the shaped window. At this size, the outline still leaves
/// room for the controls of the window.
const MIN_WINDOW: Size = Size::new(Dp::new(600.0), Dp::new(570.0));
const HOLE_RADIUS: Dp = Dp::new(28.0);
/// The largest distance between the round hole and the polygon that approximates it: half a logical pixel, because
/// the input region sent to the compositor consists of whole logical pixels.
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

    /// Starts the shaped child window and returns the pidfd of its process, which becomes
    /// readable when the process exits, or `None` while a child runs already.
    pub fn open(&mut self) -> Result<Option<OwnedFd>, Error> {
        self.reap();
        if self.process.is_some() {
            return Ok(None);
        }
        let mut command = Command::new(std::env::current_exe().map_err(Error::CurrentExecutable)?);
        let _chained_command = (command.arg("--shaped").args(&self.arguments))
            .env("LAVASHELL_INTERACTIONS_SAMPLES", self.samples.directory());
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

    /// Drops the child and marks the shaped window closed once the process of the child exited.
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

/// Ends the process of `child` and waits for it to exit.
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

/// The round hole of the shaped window, as a polygon with its corners on the circle and its edges
/// within `HOLE_TOLERANCE` of it.
pub struct Hole {
    /// The angle of each corner around the center of the circle.
    angles: Vec<f64>,
}

impl Hole {
    /// # Errors
    /// Returns an error when the number of corners needed for `HOLE_TOLERANCE` does not convert exactly to an
    /// `f64`.
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
    ui::section(ui, "04", CopyKey::Shapes, |ui| {
        ui.row(
            Modifier::new().gap(20.dp()).align_items(Align::Center),
            |ui| {
                ui.canvas(Size::new(180.dp(), 120.dp()), Modifier::new(), |d| {
                    d.fill(
                        Shape::rounded(Rect::new(0.dp(), 0.dp(), 180.dp(), 120.dp()), 12.dp()),
                        ui::BG,
                    );
                    let thumbnail = Rect::new(8.dp(), 6.dp(), 158.dp(), 100.dp());
                    let shrink = |point: Point| {
                        Point::new(
                            thumbnail.x + point.x * (thumbnail.width / WINDOW.width),
                            thumbnail.y + point.y * (thumbnail.height / WINDOW.height),
                        )
                    };
                    let points = outer(WINDOW).into_iter().map(shrink).collect::<Vec<_>>();
                    d.path(&points, true, ui::CARD, None);
                    d.path(&points, true, ui::ACCENT, Some(1.5.dp()));
                    d.circle(shrink(hole_center(WINDOW)), 5.dp(), ui::BG);
                    d.fill(
                        Shape::rounded(Rect::new(21.dp(), 17.dp(), 130.dp(), 9.dp()), 3.dp()),
                        ui::LINE,
                    );
                    d.fill(
                        Shape::rounded(Rect::new(21.dp(), 35.dp(), 103.dp(), 5.dp()), 2.dp()),
                        ui::LINE,
                    );
                    d.fill(
                        Shape::rounded(Rect::new(21.dp(), 47.dp(), 89.dp(), 5.dp()), 2.dp()),
                        ui::LINE,
                    );
                });
                ui.column(Modifier::new().gap(20.dp()).weight(1.0), |ui| {
                    ui::label(ui, CopyKey::ShapeHint, 14.sp(), ui::MUTED);
                    ui.row(Modifier::new().gap(8.dp()), |ui| {
                        let (opener, child, actions) =
                            (demo.clone(), demo.child.clone(), actions.clone());
                        ui.button(
                            Modifier::new()
                                .tag("shape_open")
                                .accessible_name(copy(CopyKey::ShapeOpen))
                                .enabled(!demo.child_open.get())
                                .border(ui::LINE, 1.dp(), 9.dp())
                                .padding(11.dp()),
                            move || {
                                let Some(window) = opener.window() else {
                                    return;
                                };
                                let outcome = child.borrow_mut().open();
                                match outcome {
                                    Ok(Some(exited)) => {
                                        let child = child.clone();
                                        actions.dispatch_when_readable(exited, window, move |_| {
                                            child.borrow_mut().reap();
                                        });
                                    }
                                    Ok(None) => {}
                                    Err(error) => report("shaped child", error),
                                }
                            },
                            |ui| ui::label(ui, CopyKey::ShapeOpen, 13.sp(), ui::INK),
                        );
                        let child = demo.child.clone();
                        ui.button(
                            Modifier::new()
                                .tag("shape_close")
                                .accessible_name(copy(CopyKey::ShapeClose))
                                .enabled(demo.child_open.get())
                                .border(ui::LINE, 1.dp(), 9.dp())
                                .padding(11.dp()),
                            move || child.borrow_mut().close(),
                            |ui| ui::label(ui, CopyKey::ShapeClose, 13.sp(), ui::INK),
                        );
                    });
                });
            },
        );
    });
}

pub fn child_compose(ui: &mut Ui, demo: &Demo, clipboard: &Clipboard, actions: &ViewActions) {
    ui.provide(
        &INTERACTION_STYLE,
        InteractionStyle {
            states: Some(demo.styles.clone()),
            caret: Some(ui::ACCENT),
            selection: Some(ui::ACCENT.with_alpha(0.2)),
            ..Default::default()
        },
        |ui| {
            ui.scroll_column(
                Modifier::new()
                    .focusable(false)
                    .gap(16.dp())
                    .fill_max_width()
                    .fill_max_height()
                    .background(ui::BG)
                    .padding(36.dp()),
                |ui| {
                    ui.text(
                        copy(CopyKey::Title),
                        Modifier::new()
                            .font_family(lavashell_example_support::DISPLAY)
                            .font_size(27.sp())
                            .font_weight(constant!(FontWeight::try_new(650.0)))
                            .color(ui::INK)
                            .semantics_with(|s| s.role = Some(Role::Heading)),
                    );
                    let quit = demo.quit.clone();
                    ui::button(ui, CopyKey::ShapeClose, Modifier::new(), move || {
                        quit.set(true);
                    });
                    crate::text::compose(ui, demo, clipboard, actions);
                    crate::transfers::compose(ui, demo, clipboard);
                },
            );
        },
    );
}
