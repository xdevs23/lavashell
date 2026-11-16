// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use lavashell_core::panel::{Extent, LayerMargins};

#[test]
fn a_new_layer_is_a_bar_across_the_top_that_reserves_its_content_height() {
    let layout = LayerSurface::new("bar").panel.current_layout();
    let bar = LayerLayout {
        margins: LayerMargins::default(),
        horizontal: AxisPlacement::Fill,
        vertical: AxisPlacement::Start(Extent::Content),
        exclusive_zone: ExclusiveZone::LayerExtent,
    };
    assert_eq!(layout, bar);
}

#[test]
fn enabling_the_keyboard_keeps_a_condition_set_before_it() {
    let waiting = LayerSurface::new("keyboard")
        .keyboard_when(|| false)
        .keyboard(true);
    let disabled = LayerSurface::new("keyboard")
        .keyboard_when(|| true)
        .keyboard(false);
    let enabled = LayerSurface::new("keyboard")
        .keyboard_when(|| true)
        .keyboard(true);
    assert_eq!(waiting.panel.keyboard.mode(), KeyboardMode::Off);
    assert_eq!(disabled.panel.keyboard.mode(), KeyboardMode::Off);
    assert_eq!(enabled.panel.keyboard.mode(), KeyboardMode::OnDemand);
}
