// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use lavashell_core::animation::SpringError;
use lavashell_core::scene::tree::ApplyError;
use lavashell_test_platform::{FrameFailure, WithoutFailures};
use std::iter::repeat_n;
use std::{cell::RefCell, rc::Rc};

/// The errors that end a test of this crate before its assertions run.
#[derive(Debug, thiserror::Error)]
pub(crate) enum TestError {
    #[error("a frame of the view failed")]
    Failed(#[from] FrameFailure),
    #[error("an update of the frame does not fit the scene tree")]
    Apply(#[from] ApplyError),
    #[error("the spring of the test cannot be built")]
    Spring(#[from] SpringError),
    #[error("a count of the test does not convert to a number")]
    Number(#[from] NumberError),
    #[error("a measured list of the test has one key in two rows")]
    DuplicateKey(#[from] DuplicateKey),
    #[error("{0} is not a density")]
    Density(f64),
    #[error("\"{0}\" is not composed")]
    NotComposed(String),
    #[error("no accessible node is named \"{0}\"")]
    NoNodeNamed(String),
    #[error("the accessibility tree has no {0}")]
    NotExported(&'static str),
    #[error("the test recorded no {0}")]
    NothingRecorded(&'static str),
}

struct Metrics;

impl TextMeasurer for Metrics {
    fn measure(&mut self, value: &str, style: &TextStyle<Dp>, width: Dp) -> Size {
        let advance: Dp = value.chars().map(|_| style.size / 2.0).sum();
        Size::new(advance.min(width), style.size)
    }
}

static BLANK: Icon = Icon {
    grid: Size::new(Dp::new(24.0), Dp::new(24.0)),
    paths: &[],
};

/// Makes one frame of `view` and applies the scene update to a new scene tree, which is dropped
/// afterwards. Fails where a component fails.
pub(crate) fn render(
    view: &mut View,
    viewport: Viewport,
    text: &mut impl TextMeasurer,
) -> Result<(), FrameFailure> {
    (view.show_frame(&mut SceneTree::default(), viewport, text)).without_failures()
}

fn composed_bounds(view: &View, tag: &str) -> Result<Rect, TestError> {
    view.bounds_of(tag)
        .ok_or_else(|| TestError::NotComposed(tag.to_owned()))
}

/// Whether `actual` and `expected` differ by at most one step of `f64` at the size of `expected`.
/// That step is the rounding error that the arithmetic in a test can add.
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
const IGNORED: Result<ActionOutcome, ActionError> = Ok(ActionOutcome::Ignored);

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

#[path = "tests/accessibility_recycling.rs"]
mod accessibility_recycling;
