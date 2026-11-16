// SPDX-License-Identifier: GPL-3.0-only
use super::{Contender, Effect, Event, Press, Recognizer, Verdict, relative};
use crate::geometry::Point;
use crate::ui::{NodeId, PointerId, PointerInput, PointerPhase};
use crate::view::View;
use crate::view::pointers::PointerSample;

pub(super) struct Raw {
    node: NodeId,
    pointer: PointerId,
    at: Option<Point>,
}

pub(super) fn declared(view: &View, press: &Press) -> Option<Box<dyn Recognizer>> {
    let node = press.node;
    let handles = view.routing.hits_of(node).any(|hit| hit.pointer.is_some());
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
        let (sample, phase) = match event {
            Event::Press(sample) => (sample, PointerPhase::Pressed),
            Event::Move(sample) => (sample, PointerPhase::Moved),
            Event::Release { sample, .. } => (sample, PointerPhase::Released),
            Event::Peer => return Vec::new(),
        };
        self.at = Some(sample.at());
        vec![self.send(phase, sample.at())]
    }

    fn cancel(&mut self) -> Vec<Effect> {
        let at = self.at.take();
        at.map(|at| self.send(PointerPhase::Cancelled, at))
            .into_iter()
            .collect()
    }
}

impl Raw {
    fn send(&self, phase: PointerPhase, at: Point) -> Effect {
        let (node, pointer) = (self.node, self.pointer);
        Box::new(move |view: &mut View| view.send_pointer_input(pointer, node, at, phase))
    }
}

impl View {
    fn send_pointer_input(
        &mut self,
        pointer: PointerId,
        node: NodeId,
        at: Point,
        phase: PointerPhase,
    ) {
        if let Some((callback, rect)) = self.handler(node, |hit| hit.pointer.clone()) {
            callback.borrow_mut()(PointerInput {
                position: relative(at, rect),
                size: rect.size(),
                phase,
                pointer,
            });
        }
    }
}
