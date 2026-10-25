// SPDX-License-Identifier: GPL-3.0-only
use super::{
    Axis, Dp, DragInput, DragPhase, Effect, Event, Hit, NodeId, Point, PointerId, PointerSample,
    Precedence, Press, Recognizer, Registration, Verdict, View, accept_along, relative,
};

pub(super) const REGISTRATION: Registration = Registration {
    precedence: Precedence(3),
    alone: false,
    declare: declared,
};

/// The recognizer of a drag along the axis declared with `on_drag` on the pressed node. The recognizer accepts the
/// pointer once the pointer moves further than the slop along that axis.
pub(super) struct Drag {
    node: NodeId,
    pointer: PointerId,
    axis: Axis,
    slop: Dp,
    /// Where the pointer was last, once the drag started.
    at: Option<Point>,
}

fn declared(view: &View, press: &Press) -> Option<Box<dyn Recognizer>> {
    let node = press.node(view);
    let axis = (view.hits_of(node)).find_map(|hit| Some(hit.drag.as_ref()?.axis))?;
    Some(Box::new(Drag {
        node,
        pointer: press.pointer(),
        axis,
        slop: press.slop,
        at: None,
    }))
}

impl Recognizer for Drag {
    fn node(&self) -> NodeId {
        self.node
    }

    fn offer(&mut self, press: PointerSample, event: Event) -> Verdict {
        accept_along(self.axis, self.slop, press, event)
    }

    /// Sends the start of the drag to the handler at the first event after the recognizer won the pointer, and the end
    /// of the drag at the release.
    fn run(&mut self, _: PointerSample, event: Event) -> Vec<Effect> {
        let (sample, released) = match event {
            Event::Press(sample) | Event::Move(sample) => (sample, false),
            Event::Release { sample, .. } => (sample, true),
            Event::Peer => return Vec::new(),
        };
        let at = sample.at();
        let phases: &[DragPhase] = match (self.at.replace(at).is_some(), released) {
            (false, false) => &[DragPhase::Started],
            (false, true) => &[DragPhase::Started, DragPhase::Ended],
            (true, false) => &[DragPhase::Moved],
            (true, true) => &[DragPhase::Ended],
        };
        phases.iter().map(|phase| self.send(*phase, at)).collect()
    }

    fn cancel(&mut self) -> Vec<Effect> {
        let at = self.at.take();
        at.map(|at| self.send(DragPhase::Cancelled, at))
            .into_iter()
            .collect()
    }
}

impl Drag {
    fn send(&self, phase: DragPhase, at: Point) -> Effect {
        let (node, pointer) = (self.node, self.pointer);
        Box::new(move |view: &mut View| view.send_drag_input(pointer, node, at, phase))
    }
}

impl View {
    fn send_drag_input(&mut self, pointer: PointerId, node: NodeId, at: Point, phase: DragPhase) {
        let drag = |hit: &Hit| Some(hit.drag.as_ref()?.handler.clone());
        if let Some((callback, rect)) = self.handler(node, drag) {
            callback.borrow_mut()(DragInput {
                position: relative(at, rect),
                size: rect.size(),
                phase,
                pointer,
            });
        }
    }
}
