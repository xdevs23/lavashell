// SPDX-License-Identifier: GPL-3.0-only
use crate::{
    animation::{Animation, AnimationError, BindError, Motion, MotionSpec},
    geometry::{Axis, Size},
    reactive::{Derived, State},
    units::{Dp, FiniteDp, NumberError},
};
use std::{cell::Cell, rc::Rc};

/// A scroll position along an axis, controlled by the application. Layout writes the viewport size,
/// the content size and the line height of the scrolling node into this state.
#[derive(Clone)]
pub struct ScrollState {
    offset: State<Dp>,
    axis: Axis,
    viewport: State<Size>,
    content: State<Size>,
    line: State<Dp>,
    animation: State<Option<Animation<Dp>>>,
    spec: Rc<Cell<MotionSpec>>,
    /// The first scroll that could not be applied to this state. The next layout of the node scrolled
    /// by this state fails with it.
    failure: Rc<Cell<Option<ScrollError>>>,
}

/// A scroll that could not be applied to a `ScrollState`, with the reason.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ScrollError {
    #[error("a scroll state cannot jump to an offset")]
    Jump(#[source] NumberError),
    #[error("a scroll state cannot scroll by a distance")]
    ScrollBy(#[source] NumberError),
    #[error("a scroll state cannot reveal a span of its content")]
    Reveal(#[source] NumberError),
    #[error("a smooth scroll cannot take an offset")]
    Smooth(#[source] AnimationError),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollView {
    pub offset: Dp,
    pub viewport: Size,
    pub content: Size,
}

impl ScrollState {
    #[must_use]
    pub fn new(axis: Axis) -> Self {
        Self {
            offset: State::new(Dp::ZERO),
            axis,
            viewport: State::new(Size::ZERO),
            content: State::new(Size::ZERO),
            line: State::new(Dp::ZERO),
            animation: State::new(None),
            spec: Rc::new(Cell::new(MotionSpec::Instant)),
            failure: Rc::default(),
        }
    }

    #[must_use]
    pub fn offset(&self) -> Dp {
        self.offset_of(self.animation.peek())
    }

    #[must_use]
    pub fn peek_offset(&self) -> Dp {
        let offset = (self.animation.peek())
            .map_or_else(|| self.offset.peek(), |animation| animation.peek());
        self.clamped(offset, self.viewport.peek(), self.content.peek())
    }

    /// The offset scrolled to, before clamping to the content. Reading it does not subscribe to the
    /// sizes written by layout.
    pub(crate) fn unclamped_offset(&self) -> Dp {
        self.unclamped(self.animation.peek())
    }

    fn offset_of(&self, animation: Option<Animation<Dp>>) -> Dp {
        self.clamped(self.unclamped(animation), self.viewport(), self.content())
    }

    fn unclamped(&self, animation: Option<Animation<Dp>>) -> Dp {
        animation.map_or_else(|| self.offset.get(), |animation| animation.get())
    }

    fn clamped(&self, offset: Dp, viewport: Size, content: Size) -> Dp {
        offset.clamp(Dp::ZERO, self.largest(viewport, content))
    }

    fn largest(&self, viewport: Size, content: Size) -> Dp {
        largest_offset(self.axis.length(viewport), self.axis.length(content))
    }

    /// A derived value computed by `select` from the scroll view. Readers are notified only when the selected value
    /// changes. The derived value also follows a smooth scroll bound to the state after this call.
    pub fn select<U: Clone + PartialEq + 'static>(
        &self,
        select: impl Fn(ScrollView) -> U + 'static,
    ) -> Derived<U> {
        let state = self.clone();
        Derived::new(move || {
            select(ScrollView {
                offset: state.offset_of(state.animation.get()),
                viewport: state.viewport(),
                content: state.content(),
            })
        })
    }

    /// Makes scrolling animate with `spec` on the presentation clock of `motion`. Wheel input during
    /// an animation adds to its target. The animation then continues from its current velocity
    /// toward the new target. A jump and a scrollbar drag move to the offset at once. An offset that
    /// the smooth scroll cannot animate to fails the next layout of the node that scrolls.
    ///
    /// # Errors
    ///
    /// Fails without a change when the scrolling is bound to the clock of another view that is
    /// still alive.
    pub fn smooth(&self, motion: &Motion, spec: impl Into<MotionSpec>) -> Result<(), BindError> {
        let animation = match self.animation.peek() {
            Some(animation) => animation,
            None => match Animation::new(self.offset.peek()) {
                Ok(animation) => {
                    self.animation.set(Some(animation.clone()));
                    animation
                }
                Err(error) => {
                    self.record(Err(ScrollError::Smooth(error)));
                    return Ok(());
                }
            },
        };
        motion.bind(&animation)?;
        self.spec.set(spec.into());
        Ok(())
    }

    /// Stores the error of `moved` for the next layout of the node that scrolls, unless an earlier
    /// error is already stored.
    fn record(&self, moved: Result<(), ScrollError>) {
        if let Err(error) = moved {
            self.failure.set(self.failure.get().or(Some(error)));
        }
    }

    #[must_use]
    pub fn axis(&self) -> Axis {
        self.axis
    }

    pub(crate) fn animation(&self) -> Option<Animation<Dp>> {
        self.animation.peek()
    }

    pub(crate) fn tracked_animation(&self) -> Option<Animation<Dp>> {
        self.animation.get()
    }

    #[must_use]
    pub fn viewport(&self) -> Size {
        self.viewport.get()
    }

    #[must_use]
    pub fn content(&self) -> Size {
        self.content.get()
    }

    /// The largest offset: how far the content extends past the viewport along the axis.
    #[must_use]
    pub fn maximum(&self) -> Dp {
        self.largest(self.viewport(), self.content())
    }

    /// The height of one line of the text in effect at the scrolling node, which is the distance
    /// scrolled by an arrow key. Zero until layout has written it.
    #[must_use]
    pub fn line(&self) -> Dp {
        self.line.get()
    }

    /// How far Page Up and Page Down scroll: the viewport along the axis less one line, so one
    /// line of the previous page remains in view.
    #[must_use]
    pub fn page(&self) -> Dp {
        page_length(self.axis.length(self.viewport()), self.line())
    }

    /// Jumps to `offset`, or to zero for a negative one. An offset that is not finite leaves the
    /// state as it is and fails the next layout of the node that scrolls.
    pub fn jump_to(&self, offset: Dp) {
        self.record(self.move_to(offset));
    }

    fn move_to(&self, offset: Dp) -> Result<(), ScrollError> {
        let offset = FiniteDp::try_new(offset)
            .map_err(ScrollError::Jump)?
            .get()
            .max(Dp::ZERO);
        self.offset.set_if_changed(offset);
        self.animation.peek().map_or(Ok(()), |animation| {
            (animation.animate_to(offset, MotionSpec::Instant)).map_err(ScrollError::Smooth)
        })
    }

    /// Scrolls by `delta`. A distance that is not finite leaves the state as it is and fails the
    /// next layout of the node that scrolls.
    pub fn scroll_by(&self, delta: Dp) {
        self.record(self.move_by(delta));
    }

    fn move_by(&self, delta: Dp) -> Result<(), ScrollError> {
        let delta = FiniteDp::try_new(delta)
            .map_err(ScrollError::ScrollBy)?
            .get();
        match self.animation.peek() {
            Some(animation) => {
                let target = (animation.target() + delta).clamp(Dp::ZERO, self.maximum());
                (animation.animate_to(target, self.spec.get())).map_err(ScrollError::Smooth)
            }
            None => self.move_to(self.offset() + delta),
        }
    }

    /// Scrolls the least distance that shows the content from `start` to `end` along the axis.
    /// A bound that is not finite leaves the state as it is and fails the next layout of the node
    /// that scrolls.
    pub fn reveal(&self, start: Dp, end: Dp) {
        self.record(self.move_to_show(start, end));
    }

    fn move_to_show(&self, start: Dp, end: Dp) -> Result<(), ScrollError> {
        let start = FiniteDp::try_new(start).map_err(ScrollError::Reveal)?;
        let end = FiniteDp::try_new(end).map_err(ScrollError::Reveal)?;
        let offset = self.offset();
        let length = self.axis.length(self.viewport());
        if start.get() < offset {
            self.move_to(start.get())
        } else if end.get() > offset + length {
            self.move_to(end.get() - length)
        } else {
            Ok(())
        }
    }

    /// Stores the sizes measured by layout and moves a target past the new largest offset back to
    /// that offset. Fails with the first scroll that could not be applied to this state since the
    /// last layout.
    pub(crate) fn publish(
        &self,
        viewport: Size,
        content: Size,
        line: Dp,
    ) -> Result<(), ScrollError> {
        self.viewport.set_if_changed(viewport);
        self.content.set_if_changed(content);
        self.line.set_if_changed(line);
        let maximum = self.largest(viewport, content);
        let target = (self.animation.peek())
            .map_or_else(|| self.offset.get(), |animation| animation.target());
        if target > maximum {
            self.jump_to(maximum);
        }
        self.failure.take().map_or(Ok(()), Err)
    }
}

/// How a row or a column scrolls: with its offset remembered by the view, or with a controlled
/// state, along the axis of the row or the column.
#[derive(Clone)]
pub enum Scrolling {
    Remembered(Axis),
    Controlled(ScrollState),
}

impl Scrolling {
    #[must_use]
    pub fn axis(&self) -> Axis {
        match self {
            Self::Remembered(axis) => *axis,
            Self::Controlled(state) => state.axis(),
        }
    }

    #[must_use]
    pub fn state(&self) -> Option<&ScrollState> {
        match self {
            Self::Remembered(_) => None,
            Self::Controlled(state) => Some(state),
        }
    }
}

impl From<Axis> for Scrolling {
    fn from(axis: Axis) -> Self {
        Self::Remembered(axis)
    }
}

impl From<ScrollState> for Scrolling {
    fn from(state: ScrollState) -> Self {
        Self::Controlled(state)
    }
}

/// The largest scroll offset for content of length `content` in a viewport of length `viewport`.
pub(crate) fn largest_offset(viewport: Dp, content: Dp) -> Dp {
    (content - viewport).max(Dp::ZERO)
}

/// The distance of one page in a viewport of length `viewport`: the viewport less one line of length
/// `line`, and at least one line.
pub(crate) fn page_length(viewport: Dp, line: Dp) -> Dp {
    (viewport - line).max(line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::animation::{Easing, Tween};
    use crate::float_asserts::assert_f64_eq;
    use crate::reactive::{Phase, Scope};
    use crate::test_error::TestResult;
    use crate::units::DpExt;
    use std::time::Duration;

    #[test]
    fn a_page_is_the_viewport_less_one_line_and_never_less_than_one_line() -> TestResult {
        let scroll = ScrollState::new(Axis::Vertical);
        let content = Size::new(100.dp(), 1000.dp());
        scroll.publish(Size::new(100.dp(), 100.dp()), content, 14.dp())?;
        assert_eq!((scroll.line(), scroll.page()), (14.dp(), 86.dp()));
        scroll.publish(Size::new(100.dp(), 10.dp()), content, 14.dp())?;
        assert_eq!(scroll.page(), 14.dp());
        Ok(())
    }

    #[test]
    fn an_offset_that_the_smooth_scroll_cannot_take_fails_the_next_layout() -> TestResult {
        let scroll = ScrollState::new(Axis::Vertical);
        let viewport = Size::new(100.dp(), 100.dp());
        let content = Size::new(100.dp(), Dp::new(f64::INFINITY));
        scroll.publish(viewport, content, Dp::ZERO)?;
        scroll.smooth(&Motion::default(), Tween::new(Duration::from_millis(100)))?;
        scroll.jump_to(Dp::new(f64::MAX));
        scroll.scroll_by(Dp::new(f64::MAX));
        assert_eq!(
            scroll.publish(viewport, content, Dp::ZERO),
            Err(ScrollError::Smooth(AnimationError::Value))
        );
        assert_eq!(scroll.publish(viewport, content, Dp::ZERO), Ok(()));
        Ok(())
    }

    #[test]
    fn a_number_that_is_not_finite_leaves_the_offset_and_fails_the_next_layout() -> TestResult {
        let scroll = ScrollState::new(Axis::Vertical);
        let viewport = Size::new(100.dp(), 100.dp());
        let content = Size::new(100.dp(), 1000.dp());
        scroll.publish(viewport, content, Dp::ZERO)?;
        scroll.jump_to(40.dp());
        let refusals: [(&dyn Fn(), ScrollError); 3] = [
            (
                &|| scroll.jump_to(Dp::new(f64::NAN)),
                ScrollError::Jump(NumberError::NotFinite),
            ),
            (
                &|| scroll.scroll_by(Dp::new(f64::INFINITY)),
                ScrollError::ScrollBy(NumberError::NotFinite),
            ),
            (
                &|| scroll.reveal(Dp::new(f64::NAN), 500.dp()),
                ScrollError::Reveal(NumberError::NotFinite),
            ),
        ];
        for (refused, error) in refusals {
            refused();
            assert_eq!(scroll.offset(), 40.dp());
            assert_eq!(scroll.publish(viewport, content, Dp::ZERO), Err(error));
        }
        assert_eq!(scroll.publish(viewport, content, Dp::ZERO), Ok(()));
        Ok(())
    }

    #[test]
    fn a_selection_follows_a_smooth_scroll_bound_after_it_was_made() -> TestResult {
        let scroll = ScrollState::new(Axis::Vertical);
        scroll.publish(
            Size::new(100.dp(), 100.dp()),
            Size::new(100.dp(), 1000.dp()),
            Dp::ZERO,
        )?;
        let row = scroll.select(|view| (view.offset / 40.dp()).floor());
        let motion = Motion::default();
        let linear = Tween::new(Duration::from_millis(100))
            .easing(Easing::Linear)
            .preserve_velocity(false);
        scroll.smooth(&motion, linear)?;
        scroll.scroll_by(100.dp());
        assert!(motion.moves_read_values());
        let s = Scope::default();
        assert_f64_eq(s.track(Phase::Composition, || row.get()), 0.0);
        motion.advance(Duration::from_millis(30));
        assert!(!s.dirty().any());
        motion.advance(Duration::from_millis(50));
        assert_f64_eq(row.get(), 1.0);
        assert!(s.dirty().any());
        motion.advance(Duration::from_millis(100));
        assert_f64_eq(row.get(), 2.0);
        Ok(())
    }
}
