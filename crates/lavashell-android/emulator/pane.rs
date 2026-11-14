// SPDX-License-Identifier: GPL-3.0-only
use crate::error::NativeMethodError;
use lavashell_actor::{Effect, Merge, Receiver};
use lavashell_android::Aosp;
use lavashell_core::prelude::*;
use lavashell_runtime::App;
use lavashell_runtime::window::Window;
use lavashell_test_fixtures::facts;

/// What the pane of a round trip shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Pane {
    Empty,
    /// The latched frame over the whole pane.
    Whole(StreamImage),
    /// The latched frame over black, inside a group that fades the frame to `opacity` and moves it right by `shift`.
    Grouped {
        image: StreamImage,
        opacity: f64,
        shift: Px,
    },
}

/// A pane replaces the pane requested before it that no frame has shown.
impl Merge for Pane {
    type Key = ();

    fn effect(&self) -> Effect<()> {
        Effect::Overwrites(())
    }
}

/// Runs the app that shows the pane requested last in a window of `size`, until the window closes. Call it inside
/// `AospHost::serve`.
///
/// # Errors
/// Fails when the window cannot be added or the run fails.
pub fn run_pane(size: Size, requested: Receiver<Pane>) -> Result<(), NativeMethodError> {
    let shown = State::new(Pane::Empty);
    let showing = shown.clone();
    // Each frame shows the pane requested before the host reported that the display is ready for that frame.
    let mut app = App::new(facts()).on_frame(move |_| {
        for pane in requested.take_arrived().into_iter().flatten() {
            showing.set(pane);
        }
    });
    let window = Window::new("Stream round trip", "org.lavashell.surfacestream")
        .size(size.width, size.height);
    (app.window(window, move |ui| compose(ui, &shown, size)))
        .map_err(NativeMethodError::AddWindow)?;
    (lavashell_runtime::run::<Aosp>(app)).map_err(NativeMethodError::Run)
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
