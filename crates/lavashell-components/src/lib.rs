// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]
//! Containers and controls that draw nothing by default. The modifiers passed by the caller and
//! the inherited styles set how they look.

use lavashell_core::numbers::whole_count;
use lavashell_core::prelude::*;
use lavashell_core::ui::Bounds;

mod holder_pool;
mod measured_recycler;
mod range;
mod recycler;
mod scroll_window;
mod scrollbar;
mod selection;
mod slider;

pub use holder_pool::{RecycledItem, RecyclerStats};
pub use measured_recycler::MeasuredRecycler;
pub use recycler::{Recycler, RecyclerItems, RecyclerLayout};
pub use selection::Selection;
pub use slider::SliderValue;

/// The error returned when a recycled list cannot scroll to an item, because a count of its rows
/// is too large to compute exactly.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("cannot reveal an item of a recycled list")]
pub struct RevealError(#[source] pub NumberError);

/// The error returned when the keys set for a measured list give two rows the same key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("cannot set the keys of a measured list: two rows have one key")]
pub struct DuplicateKey;

/// The values passed to the content of [`UiExt::scrollbar`] for drawing the thumb: the fraction
/// of the content shown in the viewport, and the scroll state that moves the thumb.
#[derive(Clone)]
pub struct Scrollbar {
    pub fraction: f64,
    scroll: ScrollState,
    /// The length of the shortest thumb: one device pixel.
    pixel: Dp,
}

impl Scrollbar {
    /// The length of the thumb on a track of length `track`: `fraction` times `track`, at least one
    /// pixel and at most `track`. A drag on the scrollbar moves a thumb of this length, also when
    /// the content does not draw one.
    #[must_use]
    pub fn thumb_length(&self, track: Dp) -> Dp {
        (track * self.fraction).max(self.pixel).min(track)
    }

    /// A modifier that moves the thumb along a track of length `track` while the scroll moves
    /// through the content. The thumb travels the length of the track minus the length of the thumb.
    pub fn follow(&self, track: Dp) -> Modifier {
        let travel = track - self.thumb_length(track);
        Modifier::new().follow_scroll(self.scroll.clone(), travel)
    }
}

/// Containers and controls that compose into a `Ui`. Each method applies `modifier` after the
/// settings of the control, so `modifier` can replace those settings.
pub trait UiExt {
    fn row(&mut self, modifier: Modifier, children: impl FnOnce(&mut Ui));

    fn column(&mut self, modifier: Modifier, children: impl FnOnce(&mut Ui));

    fn stack(&mut self, modifier: Modifier, children: impl FnOnce(&mut Ui));

    fn scroll_column(&mut self, modifier: Modifier, children: impl FnOnce(&mut Ui));

    fn button(
        &mut self,
        modifier: Modifier,
        action: impl FnMut() + 'static,
        content: impl FnOnce(&mut Ui),
    );

    fn icon_button(
        &mut self,
        icon: &'static Icon,
        options: IconOptions,
        modifier: Modifier,
        action: impl FnMut() + 'static,
    );

    fn toggle(
        &mut self,
        value: bool,
        modifier: Modifier,
        change: impl FnMut(bool) + 'static,
        content: impl FnOnce(&mut Ui, bool),
    );

    /// Composes a slider of a value from 0 to 1. The slider calls `value` each time it needs the
    /// value, so a value that changes between compositions is shown. `content` draws the slider and
    /// reads the value, clamped to 0 to 1, through the [`SliderValue`] passed to `content`. `page`
    /// is the distance by which Page Up and Page Down move the value, or `None` for a slider that
    /// ignores the page keys. A `page` that is not finite or is below zero fails the composition.
    fn slider(
        &mut self,
        value: impl Fn() -> f64 + 'static,
        page: Option<f64>,
        modifier: Modifier,
        change: impl FnMut(f64) + 'static,
        content: impl FnOnce(&mut Ui, SliderValue),
    );

    fn text_field(
        &mut self,
        value: impl Into<String>,
        options: TextFieldOptions,
        modifier: Modifier,
        change: impl FnMut(String) + 'static,
    );

    fn multiline_field(
        &mut self,
        value: impl Into<String>,
        options: TextFieldOptions,
        modifier: Modifier,
        change: impl FnMut(String) + 'static,
    );

    fn space(&mut self, modifier: Modifier);

    fn dialog(&mut self, modifier: Modifier, content: impl FnOnce(&mut Ui));

    /// Places `content` at the point `at` in the coordinates of the view. When `content` at `at`
    /// does not fit in the view, `content` moves left and up until its measured size fits, but not
    /// past the origin.
    fn popover(
        &mut self,
        at: impl Into<Deferred<Point, Bounds>>,
        modifier: Modifier,
        content: impl FnOnce(&mut Ui),
    );

    /// Composes a scrollbar that shows the position of `scroll` and moves `scroll` on drags, keys
    /// and accessibility actions. A scroll whose largest offset is not finite fails the composition.
    fn scrollbar(
        &mut self,
        scroll: ScrollState,
        modifier: Modifier,
        content: impl FnOnce(&mut Ui, Scrollbar),
    );
}

impl UiExt for Ui {
    fn row(&mut self, modifier: Modifier, children: impl FnOnce(&mut Ui)) {
        self.layout(Axis::Horizontal, modifier, children);
    }

    fn column(&mut self, modifier: Modifier, children: impl FnOnce(&mut Ui)) {
        self.layout(Axis::Vertical, modifier, children);
    }

    fn stack(&mut self, modifier: Modifier, children: impl FnOnce(&mut Ui)) {
        self.layout(Arrangement::Overlay, modifier, children);
    }

    fn scroll_column(&mut self, modifier: Modifier, children: impl FnOnce(&mut Ui)) {
        let list = Modifier::new().semantics(Semantics {
            role: Some(Role::ScrollView),
            ..Semantics::default()
        });
        self.scroll(Axis::Vertical, list.then(modifier), children);
    }

    fn button(
        &mut self,
        modifier: Modifier,
        action: impl FnMut() + 'static,
        content: impl FnOnce(&mut Ui),
    ) {
        let button = Modifier::new().on_click(action).semantics(Semantics {
            role: Some(Role::Button),
            mode: SemanticsMode::MergeDescendants,
            ..Semantics::default()
        });
        self.stack(button.then(modifier), content);
    }

    fn icon_button(
        &mut self,
        icon: &'static Icon,
        options: IconOptions,
        modifier: Modifier,
        action: impl FnMut() + 'static,
    ) {
        let button = Modifier::new().on_click(action).semantics(Semantics {
            role: Some(Role::Button),
            ..Semantics::default()
        });
        self.icon(icon, options, button.then(modifier));
    }

    fn toggle(
        &mut self,
        value: bool,
        modifier: Modifier,
        mut change: impl FnMut(bool) + 'static,
        content: impl FnOnce(&mut Ui, bool),
    ) {
        let mut semantics = Semantics {
            role: Some(Role::Switch),
            mode: SemanticsMode::MergeDescendants,
            ..Semantics::default()
        };
        semantics.state.checked = Some(if value { Checked::True } else { Checked::False });
        let switch = Modifier::new().semantics(semantics).then(modifier);
        self.button(switch, move || change(!value), |ui| content(ui, value));
    }

    fn slider(
        &mut self,
        value: impl Fn() -> f64 + 'static,
        page: Option<f64>,
        modifier: Modifier,
        change: impl FnMut(f64) + 'static,
        content: impl FnOnce(&mut Ui, SliderValue),
    ) {
        slider::compose(self, value, page, modifier, change, content);
    }

    fn text_field(
        &mut self,
        value: impl Into<String>,
        options: TextFieldOptions,
        modifier: Modifier,
        change: impl FnMut(String) + 'static,
    ) {
        let field = Modifier::new().semantics(Semantics {
            role: Some(Role::TextInput),
            ..Semantics::default()
        });
        self.text_input(value, options, field.then(modifier), change);
    }

    fn space(&mut self, modifier: Modifier) {
        let space = Modifier::new().weight(1.0);
        self.spacer(Dp::ZERO, Dp::ZERO, space.then(modifier));
    }

    fn multiline_field(
        &mut self,
        value: impl Into<String>,
        options: TextFieldOptions,
        modifier: Modifier,
        change: impl FnMut(String) + 'static,
    ) {
        let field = Modifier::new().semantics(Semantics {
            role: Some(Role::TextArea),
            ..Semantics::default()
        });
        self.text_area(value, options, field.then(modifier), change);
    }

    fn dialog(&mut self, modifier: Modifier, content: impl FnOnce(&mut Ui)) {
        let dialog = Modifier::new().modal().semantics(Semantics {
            role: Some(Role::Dialog),
            ..Semantics::default()
        });
        self.stack(dialog.then(modifier), content);
    }

    fn popover(
        &mut self,
        at: impl Into<Deferred<Point, Bounds>>,
        modifier: Modifier,
        content: impl FnOnce(&mut Ui),
    ) {
        let (popover, view) = (NodeRef::new(), self.viewport().size);
        let size: Deferred<Size, Bounds> = popover.size().into();
        let at = (at.into().zip(&size)).map(move |(at, size)| inside(at, size, view));
        let placed = Modifier::new().node_ref(&popover).position(at);
        self.stack(placed.then(modifier), content);
    }

    fn scrollbar(
        &mut self,
        scroll: ScrollState,
        modifier: Modifier,
        content: impl FnOnce(&mut Ui, Scrollbar),
    ) {
        scrollbar::compose(self, scroll, modifier, content);
    }
}

/// `ratio` as a count, for a `ratio` that is a whole number. Returns zero for a negative ratio and
/// for NaN, and `usize::MAX` for a ratio too large for `usize`.
fn count_of(ratio: f64) -> usize {
    if ratio > 0.0 {
        whole_count(ratio).unwrap_or(usize::MAX)
    } else {
        0
    }
}

/// `at` moved left and up until a box of `size` placed at that point fits in `view`, but no
/// further than the origin.
fn inside(at: Point, size: Size, view: Size) -> Point {
    let fit = |at: Dp, length: Dp, view: Dp| at.min(view - length).max(Dp::ZERO);
    Point::new(
        fit(at.x, size.width, view.width),
        fit(at.y, size.height, view.height),
    )
}

#[cfg(test)]
mod tests;
