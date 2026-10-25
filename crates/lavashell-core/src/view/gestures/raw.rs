// SPDX-License-Identifier: GPL-3.0-only
use super::{
    Effect, Event, NodeId, Point, PointerId, PointerInput, PointerPhase, PointerSample, Precedence,
    Press, Recognizer, Registration, Verdict, View, relative,
};

pub(super) const REGISTRATION: Registration = Registration {
    precedence: Precedence(1),
    alone: false,
    declare: declared,
};

/// The recognizer that sends every change of the pointer, from the press on, to the `on_pointer` handler set on the
/// innermost pressed node. The recognizer accepts the pointer at the press, together with the other recognizers
/// declared on that node.
pub(super) struct Raw {
    node: NodeId,
    pointer: PointerId,
    /// Where the pointer was last, once the recognizer won it.
    at: Option<Point>,
}

fn declared(view: &View, press: &Press) -> Option<Box<dyn Recognizer>> {
    let node = press.node(view);
    let handles = view.hits_of(node).any(|hit| hit.pointer.is_some());
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
