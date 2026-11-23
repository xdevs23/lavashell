// SPDX-License-Identifier: GPL-3.0-only
use super::{Contender, Effect, Event, Press, Recognizer, Verdict, accept_along};
use crate::geometry::Axis;
use crate::ui::{NodeId, PointerId};
use crate::units::Dp;
use crate::view::View;
use crate::view::component::index_of;
use crate::view::pointers::PointerSample;
use std::cell::RefCell;
use std::rc::Rc;

pub(super) struct Scroll {
    node: NodeId,
    axis: Axis,
    slop: Dp,
    pointer: PointerId,
    winners: Rc<RefCell<Vec<PointerId>>>,
    from: Option<Dp>,
}

pub(super) fn declared(view: &View, press: &Press) -> Option<Box<dyn Recognizer>> {
    Scroll::pressed(view, press, Rc::default()).map(|scroll| Box::new(scroll) as _)
}

impl Scroll {
    fn pressed(view: &View, press: &Press, winners: Rc<RefCell<Vec<PointerId>>>) -> Option<Self> {
        let axis = view.nodes[press.index].scroll_axis()?;
        let overflows =
            (view.scroll_maximum(press.index)).is_some_and(|maximum| maximum > Dp::ZERO);
        if !press.sample.kind().drags_content() || !overflows {
            return None;
        }
        Some(Scroll {
            node: press.node,
            axis,
            slop: press.slop,
            pointer: press.pointer(),
            winners,
            from: None,
        })
    }
}

impl Recognizer for Scroll {
    fn node(&self) -> NodeId {
        self.node
    }

    fn contender(&self) -> Contender {
        Contender::Scroll
    }

    fn joined_by(&self, view: &View, press: &Press) -> Option<Box<dyn Recognizer>> {
        if self.node != press.node {
            return None;
        }
        Scroll::pressed(view, press, self.winners.clone()).map(|joined| Box::new(joined) as _)
    }

    fn offer(&mut self, press: PointerSample, event: Event) -> Verdict {
        accept_along(self.axis, self.slop, press, event)
    }

    fn run(&mut self, press: PointerSample, event: Event) -> Vec<Effect> {
        let sample = match event {
            Event::Move { sample, .. } => sample,
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
    fn scroll_by_touch(&mut self, node: NodeId, delta: Dp) {
        if let Some(index) = index_of(&self.nodes, node) {
            self.scroll_node_by_window(index, delta);
        }
    }
}
