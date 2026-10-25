// SPDX-License-Identifier: GPL-3.0-only
use crate::{
    Error,
    copy::{CopyKey, copy},
    samples::SampleFiles,
    shape::ChildWindow,
    text::TextActions,
};
use lavashell::core::surface::SurfaceId;
use lavashell::prelude::*;
use std::{
    cell::{Cell, RefCell},
    ffi::OsString,
    rc::Rc,
};

#[derive(Clone)]
pub enum Receipt {
    Empty,
    Accepted {
        mime: String,
        bytes: usize,
        preview: Option<Image>,
    },
    Rejected,
}

/// Whether the last request to put data on the clipboard succeeded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipboardOffer {
    Offered,
    Failed,
}

impl ClipboardOffer {
    #[must_use]
    pub fn of<T, E>(result: &Result<T, E>) -> Self {
        if result.is_ok() {
            Self::Offered
        } else {
            Self::Failed
        }
    }

    /// The copy key of the status line that reports the outcome.
    #[must_use]
    pub fn status(self) -> CopyKey {
        match self {
            Self::Offered => CopyKey::ClipboardOffered,
            Self::Failed => CopyKey::ClipboardFailed,
        }
    }
}

#[derive(Clone)]
pub struct Demo {
    pub single: State<String>,
    pub multiline: State<String>,
    pub text: TextActions,
    pub receipt: State<Receipt>,
    /// `None` until the demo first puts data on the clipboard.
    pub offered: State<Option<ClipboardOffer>>,
    pub drag_over: State<bool>,
    pub state: State<InteractionState>,
    pub activations: State<u64>,
    pub styles: StateStyles,
    pub samples: Rc<SampleFiles>,
    pub child: Rc<RefCell<ChildWindow>>,
    pub child_open: State<bool>,
    pub quit: State<bool>,
    /// The surface of the demo window, set by `observe` once a frame of the window was shown.
    window: Rc<Cell<Option<SurfaceId>>>,
}

impl Demo {
    pub fn new(
        shared_samples: Option<std::path::PathBuf>,
        child_arguments: Vec<OsString>,
    ) -> Result<Self, Error> {
        let child_open = State::new(false);
        let samples = Rc::new(SampleFiles::new(shared_samples)?);
        Ok(Self {
            single: State::new(copy(CopyKey::EditableSample).into()),
            multiline: State::new(copy(CopyKey::MultilineSample).into()),
            text: TextActions::default(),
            receipt: State::new(Receipt::Empty),
            offered: State::new(None),
            drag_over: State::new(false),
            state: State::new(InteractionState::default()),
            activations: State::new(0),
            styles: crate::states::styles(),
            samples: samples.clone(),
            child: Rc::new(RefCell::new(ChildWindow::new(
                child_open.clone(),
                samples,
                child_arguments,
            ))),
            child_open,
            quit: State::new(false),
            window: Rc::default(),
        })
    }

    pub fn reset_text(&self) {
        self.single.set(copy(CopyKey::EditableSample).into());
        self.multiline.set(copy(CopyKey::MultilineSample).into());
    }

    /// The surface of the demo window, or `None` before a frame of the window was shown. View actions and
    /// clipboard reads need this surface to address the window.
    pub fn window(&self) -> Option<SurfaceId> {
        self.window.get()
    }

    pub fn observe(&self, surface: SurfaceId, view: &View) {
        self.window.set(Some(surface));
        self.text.observe(view);
        if let Some(state) = view.interaction_state("state_button") {
            self.state.set_if_changed(state);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ClipboardOffer;

    #[test]
    fn an_offer_reports_the_outcome_of_its_result() {
        assert_eq!(
            ClipboardOffer::of(&Ok::<(), ()>(())),
            ClipboardOffer::Offered
        );
        assert_eq!(
            ClipboardOffer::of(&Err::<(), ()>(())),
            ClipboardOffer::Failed
        );
    }
}
