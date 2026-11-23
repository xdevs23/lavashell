// SPDX-License-Identifier: GPL-3.0-only
use super::{SIXTY_HERTZ, TestResult, app_with_window_size};
use crate::{HostScript, NoBackend, TestPlatform};
use lavashell_core::prelude::{Density, Dp, DpExt, Modifier, Rect, Size, constant};
use lavashell_runtime::run;
use std::cell::RefCell;
use std::rc::Rc;

const FOUR_THIRDS: Density = constant!(Density::from_ratio(4.0, 3.0));
const FILLING_SPACER: &str = "filling spacer";

#[test]
fn a_window_lays_out_at_exactly_the_requested_size_without_drawing_a_frame() -> TestResult {
    let size = Size::new(1366.dp(), 768.dp());
    let filling = Modifier::new()
        .fill_max_width()
        .fill_max_height()
        .tag(FILLING_SPACER);
    let app = app_with_window_size(size, move |ui| {
        ui.spacer(Dp::ZERO, Dp::ZERO, filling.clone());
    })?;
    let bounds = Rc::new(RefCell::new(Vec::new()));
    let seen = Rc::clone(&bounds);
    let app = app.view_observer(move |_, _, _, view| {
        seen.borrow_mut().push(view.bounds_of(&FILLING_SPACER));
    });
    let script = HostScript::new(FOUR_THIRDS).record_until_rest(SIXTY_HERTZ);
    let (ran, frames) = script.serve(|host| run::<TestPlatform<NoBackend>>(app, host));
    ran?;
    assert_eq!(*bounds.borrow(), [Some(Rect::from_size(size))]);
    assert!(frames?.frames().is_empty());
    Ok(())
}
