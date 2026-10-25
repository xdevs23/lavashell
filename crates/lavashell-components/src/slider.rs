// SPDX-License-Identifier: GPL-3.0-only
use crate::UiExt;
use crate::range::RangeControl;
use lavashell_core::prelude::*;
use std::{collections::BTreeSet, rc::Rc};

/// A function that returns the value of a slider, clamped to the range 0 to 1.
pub type SliderValue = Rc<dyn Fn() -> f64>;

/// The pointers that drag a slider, and the value to restore when the last of those drags is
/// cancelled. That value is the value before the first drag started, or the end position of the
/// latest drag that ended.
#[derive(Default)]
struct Drags {
    before: f64,
    running: BTreeSet<PointerId>,
}

pub(super) fn compose(
    ui: &mut Ui,
    value: impl Fn() -> f64 + 'static,
    page: Option<f64>,
    modifier: Modifier,
    change: impl FnMut(f64) + 'static,
    content: impl FnOnce(&mut Ui, SliderValue),
) {
    let pixel = ui.viewport().pixel();
    let slider = ui.next_id();
    let drags = ui.state_of(slider, "slider-drags", Drags::default);
    let width = ui.state_of(slider, "slider-width", || Dp::ZERO);
    let published = width.clone();
    // The arrow keys move the value by one pixel of the slider width, the smallest change that is
    // visible.
    let width = width.get();
    let step = (width > Dp::ZERO).then(|| pixel / width);
    let control = RangeControl::new(value, 1.0, Axis::Horizontal, change)
        .and_then(|control| control.stepped(step, page));
    let control = match control {
        Ok(control) => control,
        Err(error) => {
            ui.fail(error);
            return;
        }
    };
    let dragged = control.clone();
    let input = Modifier::new()
        .on_measure(move |size| published.set_if_changed(size.width))
        .on_drag(Axis::Horizontal, move |event| {
            let position = event.position.x / event.size.width.max(pixel);
            let pointer = event.pointer;
            let value = match event.phase {
                DragPhase::Started => {
                    drags.update(|drags| {
                        if drags.running.is_empty() {
                            drags.before = dragged.value();
                        }
                        let _running_either_way = drags.running.insert(pointer);
                    });
                    position
                }
                DragPhase::Moved => position,
                DragPhase::Ended => {
                    drags.update(|drags| {
                        let _not_running_either_way = drags.running.remove(&pointer);
                        drags.before = position;
                    });
                    position
                }
                DragPhase::Cancelled => {
                    let last = drags
                        .update(|drags| drags.running.remove(&pointer) && drags.running.is_empty());
                    if !last {
                        return;
                    }
                    drags.with(|drags| drags.before)
                }
            };
            dragged.set(value);
        });
    let reader = control.clone();
    let range = control.modifier(Role::Slider);
    let shown: SliderValue = Rc::new(move || reader.value());
    ui.stack(input.then(range).then(modifier), |ui| content(ui, shown));
}
