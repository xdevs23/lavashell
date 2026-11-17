// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::effects::{Material, Shadow, Stroke};
use crate::frame::FrameTime;
use crate::geometry::Viewport;
use crate::input_method::{TextAction, TextHints};
use crate::reactive::State;
use crate::scene::tree::Change;
use crate::scene::{DrawCommand, TEXT_STYLE, TextStyle};
use crate::shape::{Corners, Shape};
use crate::test_error::{Exactly, Required, TestError, TestResult, WithoutFailures};
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

fn default_line_height(view: &View) -> Dp {
    let style = view.root_values.value(&TEXT_STYLE).get();
    Metrics.line_height(&style.resolve_unscaled())
}

struct CountedView {
    view: View,
    scene: Scene,
    compositions: std::rc::Rc<std::cell::Cell<u32>>,
}

const WINDOW_SIZE: Size = Size::new(Dp::new(500.0), Dp::new(300.0));

const SMALLEST: Proposal = Proposal {
    width: Some(Dp::ZERO),
    height: Some(Dp::ZERO),
};

const SHIFT_HELD: InputModifiers = InputModifiers {
    control: false,
    shift: true,
    alt: false,
};

fn frame(
    scene: &mut Scene,
    view: &mut View,
    size: Size,
    time_ms: u64,
) -> Result<Vec<Change>, TestError> {
    advance(scene, view, size, time_ms)?;
    Ok(scene.changes().to_vec())
}

fn failure_causes<T>(result: Result<T, TestError>) -> Result<Vec<ComponentError>, TestError> {
    match result {
        Ok(_) => Ok(Vec::new()),
        Err(TestError::Failed(failures)) => {
            Ok(failures.into_iter().map(|failure| failure.cause).collect())
        }
        Err(error) => Err(error),
    }
}

fn advance(scene: &mut Scene, view: &mut View, size: Size, time_ms: u64) -> Result<(), TestError> {
    let time = std::time::Duration::from_millis(time_ms);
    let timing = FrameTime {
        now: time,
        presentation: time,
        ..Default::default()
    };
    let viewport = Viewport::new(size, Density::ONE);
    advance_with_timing(scene, view, viewport, &mut Metrics, timing)
}

fn show_frame(
    scene: &mut Scene,
    view: &mut View,
    size: Size,
    time_ms: u64,
) -> Result<(), TestError> {
    advance(scene.at(time_ms), view, size, time_ms)
}

const TAB: InputEvent = InputEvent::Key {
    key: Key::Tab,
    shift: false,
};

const fn slide_offset(x: f64, y: f64) -> Result<crate::geometry::FinitePoint, NumberError> {
    crate::geometry::FinitePoint::try_new(Point::new(Dp::new(x), Dp::new(y)))
}

fn text_tag(name: &NodeName) -> Option<&'static str> {
    name.tag
        .as_ref()
        .and_then(crate::ui::NodeTag::get::<&'static str>)
        .copied()
}

fn sized() -> Modifier {
    Modifier::new().size(200.dp(), 40.dp())
}

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

fn tap<T: ViewTag + Clone>(view: &mut View, tag: &T) -> Result<(), TestError> {
    tap_at(view, view.tagged_bounds(tag)?.center());
    Ok(())
}

fn touch(point: i32) -> TouchId {
    TouchId { seat: 0, point }
}

fn finger(point: i32, at: Point) -> TouchSample {
    TouchSample::finger(touch(point), at)
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

fn painted(
    view: &mut View,
    viewport: Viewport,
    text: &mut impl TextMeasurer,
) -> Result<Vec<DrawCommand>, TestError> {
    let mut scene = Scene::default();
    advance_with(&mut scene, view, viewport, text)?;
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

#[path = "tests/failures.rs"]
mod failures;

#[path = "tests/arrivals.rs"]
mod arrivals;

#[path = "tests/bound_scroll.rs"]
mod bound_scroll;
#[path = "tests/content_sizes.rs"]
mod content_sizes;
#[path = "tests/crossings.rs"]
mod crossings;
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
#[path = "tests/sink.rs"]
mod sink;

#[path = "tests/field_lines.rs"]
mod field_lines;
#[path = "tests/first_composition.rs"]
mod first_composition;
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
