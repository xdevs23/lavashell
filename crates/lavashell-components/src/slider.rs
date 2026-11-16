// SPDX-License-Identifier: GPL-3.0-only
use crate::UiExt;
use crate::range::RangeControl;
use lavashell_core::prelude::*;
use std::{collections::BTreeSet, rc::Rc};

pub type SliderValue = Rc<dyn Fn() -> f64>;

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
            if let Some(value) =
                dragged_value(&drags, &dragged, event.phase, event.pointer, position)
            {
                dragged.set(value);
            }
        });
    let reader = control.clone();
    let range = control.modifier(Role::Slider);
    let shown: SliderValue = Rc::new(move || reader.value());
    ui.stack(input.then(range).then(modifier), |ui| content(ui, shown));
}

fn dragged_value(
    drags: &State<Drags>,
    control: &RangeControl<f64>,
    phase: DragPhase,
    pointer: PointerId,
    position: f64,
) -> Option<f64> {
    match phase {
        DragPhase::Started => {
            drags.update(|drags| {
                if drags.running.is_empty() {
                    drags.before = control.value();
                }
                let _running_either_way = drags.running.insert(pointer);
            });
            Some(position)
        }
        DragPhase::Moved => Some(position),
        DragPhase::Ended => {
            drags.update(|drags| {
                let _not_running_either_way = drags.running.remove(&pointer);
                drags.before = position;
            });
            Some(position)
        }
        DragPhase::Cancelled => {
            let last =
                drags.update(|drags| drags.running.remove(&pointer) && drags.running.is_empty());
            last.then(|| drags.with(|drags| drags.before))
        }
    }
}
