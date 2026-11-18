// SPDX-License-Identifier: GPL-3.0-only
use super::{Contender, Effect, Event, Press, Recognizer, Verdict, euclidean_distance};
use crate::ui::NodeId;
use crate::units::Dp;
use crate::view::View;
use crate::view::pointers::PointerSample;

struct ActivatingPress {
    node: NodeId,
    slop: Dp,
}

struct FocusPress {
    node: NodeId,
    slop: Dp,
}

pub(super) fn activating(view: &View, press: &Press) -> Option<Box<dyn Recognizer>> {
    let recognizer = ActivatingPress {
        node: press.node,
        slop: press.slop,
    };
    view.routing
        .node_activates(press.node)
        .then(|| Box::new(recognizer) as _)
}

pub(super) fn focusing(_: &View, press: &Press) -> Option<Box<dyn Recognizer>> {
    let recognizer = FocusPress {
        node: press.node,
        slop: press.slop,
    };
    press.innermost.then(|| Box::new(recognizer) as _)
}

fn stays_still(slop: Dp, press: PointerSample, event: Event) -> Verdict {
    match event {
        Event::Move(sample) if euclidean_distance(press.at(), sample.at()) > slop => {
            Verdict::Reject
        }
        _ => Verdict::Wait,
    }
}

impl Recognizer for ActivatingPress {
    fn node(&self) -> NodeId {
        self.node
    }

    fn contender(&self) -> Contender {
        Contender::Activate
    }

    fn offer(&mut self, press: PointerSample, event: Event) -> Verdict {
        stays_still(self.slop, press, event)
    }

    fn run(&mut self, _: PointerSample, event: Event) -> Vec<Effect> {
        match event {
            Event::Release { sample, over } if over == Some(self.node) => {
                let (node, at) = (self.node, Some(sample.at()));
                vec![Box::new(move |view: &mut View| {
                    view.activate_node(node, at);
                })]
            }
            _ => Vec::new(),
        }
    }
}

impl Recognizer for FocusPress {
    fn node(&self) -> NodeId {
        self.node
    }

    fn contender(&self) -> Contender {
        Contender::Focus
    }

    fn offer(&mut self, press: PointerSample, event: Event) -> Verdict {
        stays_still(self.slop, press, event)
    }

    fn run(&mut self, _: PointerSample, _: Event) -> Vec<Effect> {
        Vec::new()
    }
}
