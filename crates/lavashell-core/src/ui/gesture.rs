// SPDX-License-Identifier: GPL-3.0-only
use super::{ActivateAction, ContextAction, Draggable, PointerAction, TransformHandler};

#[derive(Clone)]
pub(crate) enum Gesture {
    Activate(ActivateAction),
    Context(ContextAction),
    Pointer(PointerAction),
    Drag(Draggable),
    Transform(TransformHandler),
}

#[derive(Clone, Default)]
pub(crate) struct Gestures {
    activate: Option<ActivateAction>,
    context: Option<ContextAction>,
    pointer: Option<PointerAction>,
    drag: Option<Draggable>,
    transform: Option<TransformHandler>,
}

impl Gestures {
    pub(crate) fn set(&mut self, gesture: Gesture) {
        match gesture {
            Gesture::Activate(action) => self.activate = Some(action),
            Gesture::Context(action) => self.context = Some(action),
            Gesture::Pointer(action) => self.pointer = Some(action),
            Gesture::Drag(draggable) => self.drag = Some(draggable),
            Gesture::Transform(handler) => self.transform = Some(handler),
        }
    }

    pub(crate) fn activate(&self) -> Option<&ActivateAction> {
        self.activate.as_ref()
    }

    pub(crate) fn context(&self) -> Option<&ContextAction> {
        self.context.as_ref()
    }

    pub(crate) fn pointer(&self) -> Option<&PointerAction> {
        self.pointer.as_ref()
    }

    pub(crate) fn drag(&self) -> Option<&Draggable> {
        self.drag.as_ref()
    }

    pub(crate) fn transform(&self) -> Option<&TransformHandler> {
        self.transform.as_ref()
    }

    pub(crate) fn activates(&self) -> bool {
        self.activate.is_some()
    }

    pub(crate) fn is_empty(&self) -> bool {
        !self.activates()
            && self.context.is_none()
            && self.pointer.is_none()
            && self.drag.is_none()
            && self.transform.is_none()
    }
}
