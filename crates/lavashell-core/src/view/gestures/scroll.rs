// SPDX-License-Identifier: GPL-3.0-only
use super::{
    Axis, Dp, Effect, Event, NodeId, PointerId, PointerSample, Precedence, Press, Recognizer,
    Registration, Verdict, View, accept_along, index_of,
};
use std::cell::RefCell;
use std::rc::Rc;

pub(super) const REGISTRATION: Registration = Registration {
    precedence: Precedence(4),
    alone: false,
    declare: declared,
};

/// The recognizer that scrolls a node with overflowing content when a pointer other than a mouse moves along the
/// scroll axis of the node. The recognizer accepts the pointer once the pointer moves further than the slop along
/// that axis. The recognizers of several pointers on one node share `winners`, the list of the pointers that won.
/// Only the last pointer in that list moves the content, starting at the position of that pointer when it became the
/// last one.
pub(super) struct Scroll {
    node: NodeId,
    axis: Axis,
    slop: Dp,
    pointer: PointerId,
    winners: Rc<RefCell<Vec<PointerId>>>,
    /// The position along the axis from which the next movement scrolls, set once the pointer moved further than the
    /// slop. When the pointer replaces another pointer that scrolls the same node, the first movement scrolls from the
    /// position of the pointer at that movement. Otherwise the first movement scrolls from the press point moved by
    /// the slop toward the pointer, whichever way the recognizer won the pointer, so that the content does not jump
    /// by the slop.
    from: Option<Dp>,
}

fn declared(view: &View, press: &Press) -> Option<Box<dyn Recognizer>> {
    let axis = view.nodes[press.index].scroll_axis()?;
    let overflows = (view.scroll_maximum(press.index)).is_some_and(|maximum| maximum > Dp::ZERO);
    if !press.sample.kind().drags_content() || !overflows {
        return None;
    }
    let node = press.node(view);
    let winners = (view.pressed::<Scroll>())
        .find(|scroll| scroll.node == node)
        .map_or_else(Rc::default, |scroll| scroll.winners.clone());
    Some(Box::new(Scroll {
        node,
        axis,
        slop: press.slop,
        pointer: press.pointer(),
        winners,
        from: None,
    }))
}

impl Recognizer for Scroll {
    fn node(&self) -> NodeId {
        self.node
    }

    fn offer(&mut self, press: PointerSample, event: Event) -> Verdict {
        accept_along(self.axis, self.slop, press, event)
    }

    fn run(&mut self, press: PointerSample, event: Event) -> Vec<Effect> {
        let sample = match event {
            Event::Move(sample) => sample,
            Event::Release { .. } => return self.cancel(),
            Event::Press(_) | Event::Peer => return Vec::new(),
        };
        let took_over = {
            let mut winners = self.winners.borrow_mut();
            let joins = !winners.contains(&self.pointer);
            let took_over = joins && !winners.is_empty();
            if joins {
                winners.push(self.pointer);
            }
            took_over
        };
        let at = self.axis.coordinate(sample.at());
        let moved = self.axis.signed_distance(press.at(), sample.at());
        let from = match self.from {
            Some(from) => from,
            None if took_over => at,
            None if moved.abs() > self.slop => {
                self.axis.coordinate(press.at()) + self.slop * moved.get().signum()
            }
            None => return Vec::new(),
        };
        self.from = Some(at);
        if self.winners.borrow().last() != Some(&self.pointer) {
            return Vec::new();
        }
        let (node, delta) = (self.node, from - at);
        vec![Box::new(move |view: &mut View| {
            view.scroll_by_touch(node, delta);
        })]
    }

    fn cancel(&mut self) -> Vec<Effect> {
        self.winners
            .borrow_mut()
            .retain(|pointer| *pointer != self.pointer);
        Vec::new()
    }
}

impl View {
    /// Scrolls `node` by `delta` at once, without the motion of a wheel.
    fn scroll_by_touch(&mut self, node: NodeId, delta: Dp) {
        if let Some(index) = index_of(&self.nodes, node) {
            self.scroll_node_by_window(index, delta);
        }
    }
}
