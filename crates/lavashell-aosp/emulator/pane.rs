// SPDX-License-Identifier: GPL-3.0-only
use crate::error::NativeMethodError;
use lavashell_actor::{Closed, Merge, MergeEffect, Receiver};
use lavashell_aosp::{Aosp, HostLink};
use lavashell_core::prelude::*;
use lavashell_runtime::App;
use lavashell_runtime::window::Window;
use lavashell_test_fixtures::facts;
use std::cell::Cell;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Pane {
    Empty,
    Whole(StreamImage),
    Grouped {
        image: StreamImage,
        opacity: f64,
        shift: Px,
    },
}

impl Merge for Pane {
    type Key = ();

    fn effect(&self) -> MergeEffect<()> {
        MergeEffect::Overwrites(())
    }
}

pub fn run_pane(
    size: Size,
    requested: Receiver<Pane>,
    link: HostLink,
) -> Result<(), NativeMethodError> {
    let shown = State::new(Pane::Empty);
    let showing = shown.clone();
    let closed = Rc::new(Cell::new(false));
    let closing = closed.clone();
    let mut app = App::new(facts())
        .on_frame(move |_| match requested.take_arrived() {
            Ok(panes) => panes.into_iter().for_each(|pane| showing.set(pane)),
            Err(Closed) => closing.set(true),
        })
        .exit_when(move || closed.get());
    let window = Window::new("Stream round trip", "org.lavashell.surfacestream")
        .size(size.width, size.height);
    (app.window(window, move |ui| compose(ui, &shown, size)))
        .map_err(NativeMethodError::AddWindow)?;
    (lavashell_runtime::run::<Aosp>(app, link)).map_err(NativeMethodError::Run)
}

fn compose(ui: &mut Ui, shown: &State<Pane>, size: Size) {
    let whole = Rect::from_size(size);
    let draw = move |image: StreamImage| {
        move |drawing: &mut Drawing<'_>| drawing.stream_image(&image, whole, ImageFit::Stretch)
    };
    match shown.get() {
        Pane::Empty => {}
        Pane::Whole(image) => ui.canvas(size, Modifier::new(), draw(image)),
        Pane::Grouped {
            image,
            opacity,
            shift,
        } => {
            let black = Modifier::new()
                .size(size.width, size.height)
                .background(Color::rgb(0x00_0000));
            ui.layout(Arrangement::Overlay, black, |ui| {
                let shift = ui.viewport().density.to_dp(shift);
                let moved = Modifier::new().opacity(opacity).offset(shift, Dp::ZERO);
                ui.canvas(size, moved, draw(image));
            });
        }
    }
}
