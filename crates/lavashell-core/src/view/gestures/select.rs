// SPDX-License-Identifier: GPL-3.0-only
use super::{
    Effect, Event, NodeId, PointerSample, Precedence, Press, Recognizer, Registration, Verdict,
    View,
};

pub(super) const REGISTRATION: Registration = Registration {
    precedence: Precedence(0),
    alone: false,
    declare: declared,
};

/// The recognizer that places the caret in the text of the innermost pressed node at the press, and extends the
/// selection with each move. The recognizer accepts the pointer at the press, together with the other recognizers
/// declared on that node.
pub(super) struct Select {
    node: NodeId,
    /// Whether the recognizer won the pointer.
    pressed: bool,
}

fn declared(view: &View, press: &Press) -> Option<Box<dyn Recognizer>> {
    let node = press.node(view);
    let edits = view.hits_of(node).any(|hit| hit.input.is_some());
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
                Box::new(move |view: &mut View| view.select_at(Some(node), sample.at(), false))
            }
            Event::Move(sample) => {
                Box::new(move |view: &mut View| view.select_at(Some(node), sample.at(), true))
            }
            Event::Release { sample, .. } => {
                Box::new(move |view: &mut View| view.release_text(Some(node), sample.at()))
            }
            Event::Peer => return Vec::new(),
        };
        vec![effect]
    }

    fn cancel(&mut self) -> Vec<Effect> {
        let node = self.node;
        let cancel = move |view: &mut View| {
            if let Some(edit) = view.edits.get_mut(&node) {
                edit.cancel_drag();
            }
        };
        std::mem::take(&mut self.pressed)
            .then(|| Box::new(cancel) as Effect)
            .into_iter()
            .collect()
    }
}
