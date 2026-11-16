// SPDX-License-Identifier: GPL-3.0-only
use super::{Contender, Effect, Event, Press, Recognizer, Verdict, euclidean_distance};
use crate::ui::NodeId;
use crate::units::Dp;
use crate::view::pointers::PointerSample;
use crate::view::{Hit, View};

pub(super) struct Tap {
    node: NodeId,
    slop: Dp,
    contender: Contender,
}

fn clicks(view: &View, node: NodeId) -> bool {
    view.routing.hits_of(node).any(Hit::activates)
}

pub(super) fn clicking(view: &View, press: &Press) -> Option<Box<dyn Recognizer>> {
    let tap = Tap {
        node: press.node,
        slop: press.slop,
        contender: Contender::Click,
    };
    clicks(view, press.node).then(|| Box::new(tap) as _)
}

pub(super) fn focusing(_: &View, press: &Press) -> Option<Box<dyn Recognizer>> {
    let tap = Tap {
        node: press.node,
        slop: press.slop,
        contender: Contender::Focus,
    };
    press.innermost.then(|| Box::new(tap) as _)
}

impl Recognizer for Tap {
    fn node(&self) -> NodeId {
        self.node
    }

    fn contender(&self) -> Contender {
        self.contender
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
