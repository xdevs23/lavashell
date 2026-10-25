// SPDX-License-Identifier: GPL-3.0-only
use crate::{Scrollbar, range::RangeControl};
use lavashell_core::prelude::*;
use std::collections::BTreeMap;

pub(super) fn compose(
    ui: &mut Ui,
    scroll: ScrollState,
    modifier: Modifier,
    content: impl FnOnce(&mut Ui, Scrollbar),
) {
    let pixel = ui.viewport().pixel();
    let axis = scroll.axis();
    let viewport = axis.length(scroll.viewport());
    let extent = axis.length(scroll.content());
    let maximum = scroll.maximum();
    let (current, changed) = (scroll.clone(), scroll.clone());
    let control = RangeControl::new(
        move || current.offset(),
        maximum,
        axis,
        move |offset| changed.jump_to(offset),
    )
    .and_then(|control| control.stepped(Some(scroll.line()), Some(scroll.page())));
    let control = match control {
        Ok(control) => control,
        Err(error) => {
            ui.fail(error);
            return;
        }
    };
    let bar = Scrollbar {
        fraction: if extent > Dp::ZERO {
            (viewport / extent).min(1.0)
        } else {
            1.0
        },
        scroll,
        pixel,
    };
    let (pointer, thumbs) = (control.clone(), bar.clone());
    let track = ui.next_id();
    let grabs = ui.state_of(track, "scrollbar-grips", BTreeMap::<PointerId, Dp>::new);
    let drag = Modifier::new().on_drag(axis, move |event| {
        let length = axis.length(event.size).max(pixel);
        let position = axis.coordinate(event.position);
        let thumb = thumbs.thumb_length(length);
        let scrolled = if maximum > Dp::ZERO {
            pointer.value() / maximum
        } else {
            0.0
        };
        let start = (length - thumb) * scrolled;
        let grabbed = grabs.with(|grabs| grabs.get(&event.pointer).copied());
        let offset = grabbed.unwrap_or_else(|| {
            let thumb_middle = thumb / 2.0;
            let offset = if position >= start && position <= start + thumb {
                position - start
            } else {
                thumb_middle
            };
            let _no_grip_before = grabs.update(|grabs| grabs.insert(event.pointer, offset));
            offset
        });
        pointer.set(maximum * ((position - offset) / (length - thumb).max(pixel)).clamp(0.0, 1.0));
        if matches!(event.phase, DragPhase::Ended | DragPhase::Cancelled) {
            let _the_grip_ends_with_the_drag = grabs.update(|grabs| grabs.remove(&event.pointer));
        }
    });
    let range = control.modifier(Role::ScrollBar);
    ui.layout(
        Arrangement::Overlay,
        drag.then(range).then(modifier),
        |ui| {
            content(ui, bar);
        },
    );
}
