// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use lavashell_core::accessibility::AccessibleNode;
use lavashell_core::animation::SpringError;
use lavashell_core::frame::FrameTime;
use lavashell_core::ui::{NodeTag, ViewTag};
use lavashell_test_fixtures::{FrameFailure, RecordedScene, WithoutFailures};
use std::iter::repeat_n;
use std::time::Duration;
use std::{cell::RefCell, rc::Rc};

#[derive(Debug, thiserror::Error)]
pub(crate) enum TestError {
    #[error("a frame of the view failed")]
    Failed(#[from] FrameFailure),
    #[error("the spring of the test cannot be built")]
    Spring(#[from] SpringError),
    #[error("a count of the test does not convert to a number")]
    Number(#[from] NumberError),
    #[error("a measured list of the test has one key in two rows")]
    DuplicateKey(#[from] DuplicateKey),
    #[error("{0} is not a density")]
    Density(f64),
    #[error("{0:?} is not composed")]
    NotComposed(NodeTag),
    #[error("no accessible node is named \"{0}\"")]
    NoNodeNamed(String),
    #[error("the accessibility tree has no {0}")]
    NotExported(&'static str),
    #[error("the test recorded no {0}")]
    NothingRecorded(&'static str),
}

struct HalfEmMetrics;

impl TextMeasurer for HalfEmMetrics {
    fn measure(&mut self, value: &str, style: &TextStyle<Dp>, width: Dp) -> Size {
        let advance: Dp = value.chars().map(|_| style.size / 2.0).sum();
        Size::new(advance.min(width), style.size)
    }
}

static BLANK: Icon = Icon {
    grid: Size::new(Dp::new(24.0), Dp::new(24.0)),
    paths: &[],
};

pub(crate) fn render(
    view: &mut View,
    viewport: Viewport,
    text: &mut impl TextMeasurer,
) -> Result<(), FrameFailure> {
    (view.frame_into(viewport, text, &mut SceneBuilder::default())).without_failures()
}

fn frame_at(
    view: &mut View,
    scene: &mut RecordedScene,
    viewport: Viewport,
    ms: u64,
) -> Result<(), FrameFailure> {
    let time = Duration::from_millis(ms);
    let timing = FrameTime {
        now: time,
        presentation: time,
        ..FrameTime::default()
    };
    (view.frame_into_at(viewport, &mut HalfEmMetrics, timing, scene)).without_failures()
}

fn named<'a>(view: &'a View, name: &str) -> Result<&'a AccessibleNode, TestError> {
    view.accessibility()
        .nodes
        .values()
        .find(|node| node.name.as_deref() == Some(name))
        .ok_or_else(|| TestError::NoNodeNamed(name.to_owned()))
}

fn act(
    view: &mut View,
    target: NodeId,
    action: AccessibilityAction,
) -> Result<ActionOutcome, ActionError> {
    view.accessibility_action(ActionRequest { target, action })
}

fn key(view: &mut View, key: Key) {
    view.input(InputEvent::Key { key, shift: false });
}

fn composed_bounds<T: ViewTag + Clone>(view: &View, tag: &T) -> Result<Rect, TestError> {
    view.bounds_of(tag)
        .ok_or_else(|| TestError::NotComposed(NodeTag::new(tag.clone())))
}

fn is_close(actual: f64, expected: f64) -> bool {
    (actual - expected).abs() <= f64::EPSILON * expected.abs().max(1.0)
}

pub(crate) fn assert_close(actual: f64, expected: f64) {
    assert!(is_close(actual, expected), "{actual} is not {expected}");
}

fn click(view: &mut View, x: Dp, y: Dp) {
    view.input(InputEvent::Press(Point { x, y }));
    view.input(InputEvent::Release(Point { x, y }));
}

const PERFORMED: Result<ActionOutcome, ActionError> = Ok(ActionOutcome::Performed);

#[path = "tests/controls.rs"]
mod controls;

#[path = "tests/recycling.rs"]
mod recycling;

#[path = "tests/scrollbar_follow.rs"]
mod scrollbar_follow;

#[path = "tests/scroll_focus.rs"]
mod scroll_focus;

#[path = "tests/modals.rs"]
mod modals;

#[path = "tests/accessibility_controls.rs"]
mod accessibility_controls;

#[path = "tests/accessibility_ranges.rs"]
mod accessibility_ranges;

#[path = "tests/accessibility_recycling.rs"]
mod accessibility_recycling;
