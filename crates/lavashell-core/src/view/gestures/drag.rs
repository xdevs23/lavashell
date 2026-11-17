// SPDX-License-Identifier: GPL-3.0-only
use super::{Contender, Effect, Event, Press, Recognizer, Verdict, accept_along, relative};
use crate::geometry::{Axis, Point};
use crate::ui::{DragInput, DragPhase, Gestures, NodeId, PointerId};
use crate::units::Dp;
use crate::view::View;
use crate::view::pointers::PointerSample;

pub(super) struct Drag {
    node: NodeId,
    pointer: PointerId,
    axis: Axis,
    slop: Dp,
    at: Option<Point>,
}

pub(super) fn declared(view: &View, press: &Press) -> Option<Box<dyn Recognizer>> {
    let node = press.node;
    let axis = (view.routing.hits_of(node)).find_map(|hit| Some(hit.gestures.drag()?.axis))?;
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

    fn contender(&self) -> Contender {
        Contender::Drag
    }

    fn offer(&mut self, press: PointerSample, event: Event) -> Verdict {
        accept_along(self.axis, self.slop, press, event)
    }

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
        if let Some((draggable, rect)) = self.handler(node, Gestures::drag) {
            draggable.handler.borrow_mut()(DragInput {
                position: relative(at, rect),
                size: rect.size(),
                phase,
                pointer,
            });
        }
    }
}
