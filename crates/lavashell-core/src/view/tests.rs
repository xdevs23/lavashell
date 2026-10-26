// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::effects::{Material, Shadow, Stroke};
use crate::input_method::{TextAction, TextHints};
use crate::reactive::State;
use crate::shape::{Corners, Shape};
use crate::test_error::{Exactly, Required, TestError, TestResult};
use crate::ui::{Arrangement, Modifier, TextFieldOptions};
use crate::units::{Density, DpExt, NumberError, Positive, SpExt};
use test_scene::{Scene, advance_with, advance_with_timing};
use test_tags::Tagged;
use test_text::repeated;

struct Metrics;

impl TextMeasurer for Metrics {
    fn measure(&mut self, t: &str, s: &TextStyle<Dp>, w: Dp) -> Size {
        Size::new(repeated(s.size * 0.5, t.len()).min(w), s.size * 1.3)
    }
}

const WINDOW_SIZE: Size = Size::new(Dp::new(500.0), Dp::new(300.0));

fn frame(view: &mut View, size: Size, time_ms: u64) -> Result<SceneUpdate, FrameError> {
    let time = std::time::Duration::from_millis(time_ms);
    view.frame_at(
        Viewport::new(size, Density::ONE),
        &mut Metrics,
        FrameTime {
            now: time,
            presentation: time,
            ..Default::default()
        },
    )
}

/// Paints a frame of `view` at `time_ms` and applies the resulting scene update to `scene`.
fn advance(scene: &mut Scene, view: &mut View, size: Size, time_ms: u64) -> Result<(), TestError> {
    scene.show(frame(view, size, time_ms)?)?;
    Ok(())
}

/// Paints a frame of `view` at `time_ms` and applies the resulting scene update to `scene`. `scene`
/// then evaluates its animated slots at `time_ms`, the time at which the frame is presented.
fn show_frame(
    scene: &mut Scene,
    view: &mut View,
    size: Size,
    time_ms: u64,
) -> Result<(), TestError> {
    let update = frame(view, size, time_ms)?;
    scene.at(time_ms).show(update)?;
    Ok(())
}

const TAB: InputEvent = InputEvent::Key {
    key: Key::Tab,
    shift: false,
};

/// The offset by which a slide transition moves a node: `x` and `y` dp, checked to be finite.
const fn slide_offset(x: f64, y: f64) -> Result<crate::geometry::FinitePoint, NumberError> {
    crate::geometry::FinitePoint::try_new(Point::new(Dp::new(x), Dp::new(y)))
}

fn center(rect: Rect) -> Point {
    Point::new(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0)
}

fn sized() -> Modifier {
    Modifier::new().size(200.dp(), 40.dp())
}

/// The options of a text field whose input method shows `action` on its Enter key.
fn acting(action: TextAction) -> TextFieldOptions {
    TextFieldOptions {
        hints: TextHints {
            action: Some(action),
            ..TextHints::default()
        },
        ..TextFieldOptions::default()
    }
}

fn tap_at(view: &mut View, at: Point) {
    view.input(InputEvent::Press(at));
    view.input(InputEvent::Release(at));
}

fn tap(view: &mut View, tag: &str) -> Result<(), TestError> {
    tap_at(view, center(view.tagged_bounds(tag)?));
    Ok(())
}

fn touch(point: i32) -> TouchId {
    TouchId { seat: 0, point }
}

fn counting_enter(count: State<u32>) -> impl FnMut(Shortcut) -> bool + 'static {
    move |shortcut| {
        let enter = shortcut == Shortcut::key(Key::Enter);
        if enter {
            count.update(|entered| *entered += 1);
        }
        enter
    }
}

/// The draw commands of `update` applied to an empty scene. `update` must be the first update of a
/// view, because only the first update contains the whole scene and later updates contain only
/// changes.
fn painted(update: SceneUpdate) -> Result<Vec<DrawCommand>, ApplyError> {
    let mut scene = Scene::default();
    scene.show(update)?;
    Ok(scene.commands())
}

fn stroke_widths(commands: &[DrawCommand]) -> Vec<Dp> {
    (commands.iter())
        .filter_map(|command| match command {
            DrawCommand::Stroke { stroke, .. } => Some(stroke.width.get()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_component_that_fails_its_composition_fails_the_frame() {
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        ui.scoped("counted", Modifier::new(), |ui| {
            ui.fail(NumberError::OutOfRange);
        });
    });
    assert!(matches!(
        frame(&mut view, WINDOW_SIZE, 0),
        Err(FrameError::Layout(LayoutError::Composition(
            CompositionError::Number(NumberError::OutOfRange)
        )))
    ));
}

#[path = "tests/bound_scroll.rs"]
mod bound_scroll;
#[path = "tests/content_sizes.rs"]
mod content_sizes;
#[path = "tests/deferred.rs"]
mod deferred;
#[path = "tests/editing.rs"]
mod editing;
#[path = "tests/layout_cycles.rs"]
mod layout_cycles;

#[path = "tests/layout_input.rs"]
mod layout_input;
#[path = "tests/visual_scale.rs"]
mod visual_scale;

#[path = "tests/locals.rs"]
mod locals;

#[path = "tests/state_scroll.rs"]
mod state_scroll;

#[path = "tests/motion.rs"]
mod motion;

#[path = "tests/color_motion.rs"]
mod color_motion;

#[path = "tests/exits.rs"]
mod exits;

#[path = "tests/groups.rs"]
mod groups;

#[path = "tests/field_lines.rs"]
mod field_lines;
#[path = "tests/icons.rs"]
mod icons;
#[path = "tests/invalid_values.rs"]
mod invalid_values;
#[path = "tests/material_group.rs"]
mod material_group;
#[path = "tests/material_group_motion.rs"]
mod material_group_motion;
#[path = "tests/material_motion.rs"]
mod material_motion;
#[path = "tests/moving_hits.rs"]
mod moving_hits;
#[path = "tests/presence.rs"]
mod presence;
#[path = "tests/probe.rs"]
mod probe;
#[path = "tests/probe_effects.rs"]
mod probe_effects;
#[path = "tests/reflow.rs"]
mod reflow;
#[path = "tests/shared_targets.rs"]
mod shared_targets;
#[path = "tests/shown_when.rs"]
mod shown_when;
#[path = "tests/size_range.rs"]
mod size_range;
#[path = "tests/slots.rs"]
mod slots;
#[path = "tests/smallest.rs"]
mod smallest;
#[path = "tests/weights.rs"]
mod weights;

#[path = "tests/focus_visible.rs"]
mod focus_visible;
#[path = "tests/interaction.rs"]
mod interaction;
#[path = "tests/selected.rs"]
mod selected;
#[path = "tests/selection.rs"]
mod selection;
#[path = "tests/state_appearance.rs"]
mod state_appearance;
#[path = "tests/state_outline.rs"]
mod state_outline;

#[path = "tests/selection_source.rs"]
mod selection_source;

#[path = "tests/paste_snapshot.rs"]
mod paste_snapshot;

#[path = "tests/native_text_move.rs"]
mod native_text_move;

#[path = "tests/state_binding.rs"]
mod state_binding;

#[path = "tests/protected.rs"]
mod protected;

#[path = "tests/wheel.rs"]
mod wheel;

#[path = "tests/pointers.rs"]
mod pointers;

#[path = "tests/gestures.rs"]
mod gestures;
#[path = "tests/touch_slop.rs"]
mod touch_slop;

#[path = "tests/pinch.rs"]
mod pinch;

#[path = "tests/transform_over_keys.rs"]
mod transform_over_keys;

#[path = "tests/sideways.rs"]
mod sideways;

#[path = "tests/timers.rs"]
mod timers;

#[path = "tests/text_actions.rs"]
mod text_actions;

#[path = "tests/text_session.rs"]
mod text_session;

#[path = "tests/text_commands.rs"]
mod text_commands;
