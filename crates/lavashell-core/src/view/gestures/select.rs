// SPDX-License-Identifier: GPL-3.0-only
use super::{Contender, Effect, Event, Press, Recognizer, Verdict};
use crate::ui::NodeId;
use crate::view::View;
use crate::view::pointers::PointerSample;

pub(super) struct Select {
    node: NodeId,
    pressed: bool,
}

pub(super) fn declared(view: &View, press: &Press) -> Option<Box<dyn Recognizer>> {
    let node = press.node;
    let edits = view.routing.hits_of(node).any(|hit| hit.input.is_some());
    let select = Select {
        node,
        pressed: false,
    };
    (press.innermost && edits).then(|| Box::new(select) as _)
}

impl Recognizer for Select {
    fn node(&self) -> NodeId {
        self.node
    }

    fn contender(&self) -> Contender {
        Contender::Select
    }

    fn offer(&mut self, _: PointerSample, event: Event) -> Verdict {
        match event {
            Event::Press(_) => Verdict::AcceptWithNode,
            _ => Verdict::Wait,
        }
    }

    fn run(&mut self, _: PointerSample, event: Event) -> Vec<Effect> {
        let node = self.node;
        let effect: Effect = match event {
            Event::Press(sample) => {
                self.pressed = true;
                Box::new(move |view: &mut View| view.press_text_at(node, sample.at()))
            }
            Event::Move { sample, .. } => {
                Box::new(move |view: &mut View| view.drag_text_to(node, sample.at()))
            }
            Event::Release { sample, .. } => {
                Box::new(move |view: &mut View| view.release_text(node, sample.at()))
            }
            Event::Peer => return Vec::new(),
        };
        vec![effect]
    }

    fn cancel(&mut self) -> Vec<Effect> {
        let node = self.node;
        let cancel = move |view: &mut View| {
            if let Some(edit) = view.editing.edits.get_mut(&node) {
                edit.cancel_drag();
            }
        };
        std::mem::take(&mut self.pressed)
            .then(|| Box::new(cancel) as Effect)
            .into_iter()
            .collect()
    }
}
