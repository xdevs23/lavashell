// SPDX-License-Identifier: GPL-3.0-only
use crate::destination::Destination;
use skia_safe::Canvas;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Default)]
pub(crate) struct Scopes(Rc<RefCell<Frames>>);

#[must_use]
pub(crate) struct EnteredScope(());

#[must_use]
pub(crate) struct SavedScope<'c> {
    frames: Rc<RefCell<Frames>>,
    canvas: &'c Canvas,
    restore_to: usize,
}

#[derive(Default)]
struct Frames(Vec<Frame>);

struct Frame {
    canvas: usize,
    opaque: bool,
    saved: Vec<bool>,
}

fn change_frames<R>(frames: &RefCell<Frames>, change: impl FnOnce(&mut Frames) -> R) -> R {
    change(&mut frames.borrow_mut())
}

impl Scopes {
    pub fn enter(&self, canvas: &Canvas) -> EnteredScope {
        let identity = std::ptr::from_ref(canvas).addr();
        change_frames(&self.0, |frames| {
            let opaque = frames
                .0
                .last()
                .is_none_or(|parent| parent.canvas == identity && parent.opaque);
            frames.0.push(Frame {
                canvas: identity,
                opaque,
                saved: Vec::new(),
            });
        });
        EnteredScope(())
    }

    pub fn leave(&self, EnteredScope(()): EnteredScope) {
        let _left_frame_is_not_needed = change_frames(&self.0, |frames| frames.0.pop());
    }

    pub fn opaque(&self) -> bool {
        change_frames(&self.0, |frames| {
            frames.0.last().is_some_and(|frame| frame.opaque)
        })
    }

    pub fn open(&self, destination: Destination) {
        change_frames(&self.0, |frames| frames.open(destination));
    }

    pub fn close(&self) {
        change_frames(&self.0, Frames::close);
    }

    pub fn save<'c>(&self, canvas: &'c Canvas, destination: Destination) -> SavedScope<'c> {
        self.open(destination);
        SavedScope {
            frames: Rc::clone(&self.0),
            canvas,
            restore_to: canvas.save(),
        }
    }
}

impl Drop for SavedScope<'_> {
    fn drop(&mut self) {
        let _canvas_returned_for_chaining = self.canvas.restore_to_count(self.restore_to);
        change_frames(&self.frames, Frames::close);
    }
}

impl Frames {
    fn open(&mut self, destination: Destination) {
        if let Some(frame) = self.0.last_mut() {
            frame.saved.push(frame.opaque);
            frame.opaque &= destination == Destination::Surface;
        }
    }

    fn close(&mut self) {
        if let Some(frame) = self.0.last_mut()
            && let Some(opaque) = frame.saved.pop()
        {
            frame.opaque = opaque;
        }
    }
}
