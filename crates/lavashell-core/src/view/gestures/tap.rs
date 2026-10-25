// SPDX-License-Identifier: GPL-3.0-only
use super::{
    Dp, Effect, Event, Hit, NodeId, PointerSample, Precedence, Press, Recognizer, Registration,
    Verdict, View, euclidean_distance,
};

/// A tap on a node with an action that a click runs.
pub(super) const CLICK: Registration = Registration {
    precedence: Precedence(2),
    alone: false,
    declare: clicking,
};

/// A tap on the innermost pressed node when that node declares no other recognizer. The tap wins the pointer at a
/// release without movement, and `View::focus_winner` then moves the focus to the node.
pub(super) const FOCUS: Registration = Registration {
    precedence: Precedence(6),
    alone: true,
    declare: focusing,
};

/// The recognizer of a press that does not move further than the slop. The tap never accepts the pointer by itself.
/// It wins the pointer when it is the only recognizer left, or at the release when it is the innermost recognizer
/// left. It then activates the node when the pointer was released over that node.
pub(super) struct Tap {
    node: NodeId,
    slop: Dp,
}

fn clicks(view: &View, node: NodeId) -> bool {
    view.hits_of(node).any(Hit::activates)
}

fn clicking(view: &View, press: &Press) -> Option<Box<dyn Recognizer>> {
    let node = press.node(view);
    let slop = press.slop;
    clicks(view, node).then(|| Box::new(Tap { node, slop }) as _)
}

fn focusing(view: &View, press: &Press) -> Option<Box<dyn Recognizer>> {
    let node = press.node(view);
    let slop = press.slop;
    press.innermost.then(|| Box::new(Tap { node, slop }) as _)
}

impl Recognizer for Tap {
    fn node(&self) -> NodeId {
        self.node
    }

    fn offer(&mut self, press: PointerSample, event: Event) -> Verdict {
        match event {
            Event::Move(sample) if euclidean_distance(press.at(), sample.at()) > self.slop => {
                Verdict::Reject
            }
            _ => Verdict::Wait,
        }
    }

    fn run(&mut self, _: PointerSample, event: Event) -> Vec<Effect> {
        match event {
            Event::Release { sample, over } if over == Some(self.node) => {
                let (node, at) = (self.node, sample.at());
                vec![Box::new(move |view: &mut View| view.activate_hit(node, at))]
            }
            _ => Vec::new(),
        }
    }
}
