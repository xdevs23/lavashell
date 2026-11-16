// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

use lavashell_core::prelude::*;
use lavashell_core::ui::Bounds;

mod holder_pool;
mod measured_recycler;
mod popover;
mod range;
mod recycler;
mod scroll_window;
mod scrollbar;
mod selection;
mod slider;

pub use holder_pool::{Generation, HolderId, RecycledItem, RecyclerStats, ViewTypeId};
pub use measured_recycler::MeasuredRecycler;
pub use recycler::{Recycler, RecyclerItems, RecyclerLayout};
pub use selection::Selection;
pub use slider::SliderValue;

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("cannot reveal an item of a recycled list")]
pub struct RevealError(#[source] pub NumberError);

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("cannot set the keys of a measured list: two rows have one key")]
pub struct DuplicateKey;

#[derive(Clone)]
pub struct Scrollbar {
    pub fraction: f64,
    scroll: ScrollState,
    pixel: Dp,
}

impl Scrollbar {
    #[must_use]
    pub fn thumb_length(&self, track: Dp) -> Dp {
        (track * self.fraction).max(self.pixel).min(track)
    }

    pub fn follow(&self, track: Dp) -> Modifier {
        let travel = track - self.thumb_length(track);
        Modifier::new().follow_scroll(self.scroll.clone(), travel)
    }
}

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

    fn popover(
        &mut self,
        at: impl Into<Deferred<Point, Bounds>>,
        modifier: Modifier,
        content: impl FnOnce(&mut Ui),
    );

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
        popover::compose(self, at, modifier, content);
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

#[cfg(test)]
mod tests;
