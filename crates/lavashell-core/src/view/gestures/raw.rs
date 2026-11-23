// SPDX-License-Identifier: GPL-3.0-only
use super::{Contender, Effect, Event, Press, Recognizer, Verdict, relative};
use crate::geometry::Point;
use crate::ui::{Gestures, NodeId, PointerId, PointerInput, PointerPhase};
use crate::view::pointers::PointerSample;
use crate::view::{InputTime, View};

pub(super) struct Raw {
    node: NodeId,
    pointer: PointerId,
    at: Option<Point>,
}

pub(super) fn declared(view: &View, press: &Press) -> Option<Box<dyn Recognizer>> {
    let node = press.node;
    let handles = (view.routing.hits_of(node)).any(|hit| hit.gestures.pointer().is_some());
    let raw = Raw {
        node,
        pointer: press.pointer(),
        at: None,
    };
    (press.innermost && handles).then(|| Box::new(raw) as _)
}

impl Recognizer for Raw {
    fn node(&self) -> NodeId {
        self.node
    }

    fn contender(&self) -> Contender {
        Contender::Raw
    }

    fn offer(&mut self, _: PointerSample, event: Event) -> Verdict {
        match event {
            Event::Press(_) => Verdict::AcceptWithNode,
            _ => Verdict::Wait,
        }
    }

    fn run(&mut self, _: PointerSample, event: Event) -> Vec<Effect> {
        let (sample, phase, time) = match event {
            Event::Press(sample) => (sample, PointerPhase::Pressed, None),
            Event::Move { sample, time } => (sample, PointerPhase::Moved, time),
            Event::Release { sample, .. } => (sample, PointerPhase::Released, None),
            Event::Peer => return Vec::new(),
        };
        self.at = Some(sample.at());
        vec![self.send(phase, sample.at(), time)]
    }

    fn cancel(&mut self) -> Vec<Effect> {
        let at = self.at.take();
        at.map(|at| self.send(PointerPhase::Cancelled, at, None))
            .into_iter()
            .collect()
    }
}

impl Raw {
    fn send(&self, phase: PointerPhase, at: Point, time: Option<InputTime>) -> Effect {
        let (node, pointer) = (self.node, self.pointer);
        Box::new(move |view: &mut View| view.send_pointer_input(pointer, node, at, phase, time))
    }
}

impl View {
    fn send_pointer_input(
        &mut self,
        pointer: PointerId,
        node: NodeId,
        at: Point,
        phase: PointerPhase,
        time: Option<InputTime>,
    ) {
        if let Some((callback, rect)) = self.handler(node, Gestures::pointer) {
            callback.borrow_mut()(PointerInput {
                position: relative(at, rect),
                size: rect.size(),
                phase,
                pointer,
                time,
            });
        }
    }
}
