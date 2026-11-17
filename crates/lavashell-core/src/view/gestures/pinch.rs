// SPDX-License-Identifier: GPL-3.0-only
use super::{
    Contender, Effect, Event, Press, Recognizer, Verdict, euclidean_distance, midpoint, relative,
};
use crate::geometry::Point;
use crate::ui::{DragPhase, Gestures, NodeId, PointerId, TransformInput};
use crate::units::Dp;
use crate::view::View;
use crate::view::pointers::PointerSample;
use std::cell::RefCell;
use std::rc::Rc;

struct Pinch {
    node: NodeId,
    slop: Dp,
    phase: Phase,
}

#[derive(Clone, Copy)]
struct Contact {
    pointer: PointerId,
    at: Point,
    won: bool,
}

enum Phase {
    One(Contact),
    Two {
        first: Contact,
        second: Contact,
        pressed: Dp,
        claimed: bool,
    },
    Started {
        first: Contact,
        second: Contact,
        start: Dp,
    },
}

pub(super) struct PinchPoint {
    pinch: Rc<RefCell<Pinch>>,
    pointer: PointerId,
}

pub(super) fn declared(view: &View, press: &Press) -> Option<Box<dyn Recognizer>> {
    let node = press.node;
    if !transformable(view, node) || !press.sample.kind().drags_content() {
        return None;
    }
    let pointer = press.pointer();
    let first = Contact {
        pointer,
        at: press.sample.at(),
        won: false,
    };
    let pinch = Pinch {
        node,
        slop: press.slop,
        phase: Phase::One(first),
    };
    Some(Box::new(PinchPoint {
        pinch: Rc::new(RefCell::new(pinch)),
        pointer,
    }))
}

fn transformable(view: &View, node: NodeId) -> bool {
    view.routing
        .hits_of(node)
        .any(|hit| hit.gestures.transform().is_some())
}

impl Recognizer for PinchPoint {
    fn node(&self) -> NodeId {
        self.pinch.borrow().node
    }

    fn contender(&self) -> Contender {
        Contender::Pinch
    }

    fn joined_by(&self, _view: &View, press: &Press) -> Option<Box<dyn Recognizer>> {
        let pinch = self.pinch.borrow();
        let pointer = press.pointer();
        let alone = matches!(pinch.phase, Phase::One(first) if first.pointer != pointer);
        let joined = Self {
            pinch: self.pinch.clone(),
            pointer,
        };
        (alone && pinch.node == press.node).then(|| Box::new(joined) as _)
    }

    fn offer(&mut self, _: PointerSample, event: Event) -> Verdict {
        let mut pinch = self.pinch.borrow_mut();
        pinch.follow(self.pointer, event);
        match pinch.phase {
            Phase::Two { claimed: true, .. } => Verdict::Accept,
            _ => Verdict::Wait,
        }
    }

    fn run(&mut self, _: PointerSample, event: Event) -> Vec<Effect> {
        let mut pinch = self.pinch.borrow_mut();
        let started = matches!(pinch.phase, Phase::Started { .. });
        pinch.follow(self.pointer, event);
        let mut effects = pinch.win(self.pointer);
        effects.extend(match event {
            Event::Release { .. } => pinch.leave(self.pointer, DragPhase::Ended),
            Event::Move(_) if started => pinch.report(DragPhase::Moved),
            Event::Press(_) | Event::Move(_) | Event::Peer => Vec::new(),
        });
        effects
    }

    fn cancel(&mut self) -> Vec<Effect> {
        self.pinch
            .borrow_mut()
            .leave(self.pointer, DragPhase::Cancelled)
    }

    fn challenge(&mut self) -> bool {
        true
    }

    fn declared(&self, view: &View) -> bool {
        transformable(view, self.pinch.borrow().node)
    }
}

impl Pinch {
    fn follow(&mut self, pointer: PointerId, event: Event) {
        let sample = match event {
            Event::Press(sample) | Event::Move(sample) | Event::Release { sample, .. } => sample,
            Event::Peer => return,
        };
        match &mut self.phase {
            Phase::One(first) if first.pointer == pointer => first.at = sample.at(),
            Phase::One(first) if matches!(event, Event::Press(_)) => {
                let second = Contact {
                    pointer,
                    at: sample.at(),
                    won: false,
                };
                self.phase = Phase::Two {
                    first: *first,
                    second,
                    pressed: euclidean_distance(first.at, second.at),
                    claimed: false,
                };
            }
            Phase::One(_) => {}
            Phase::Two {
                first,
                second,
                pressed,
                claimed,
            } => {
                contact(first, second, pointer).at = sample.at();
                let changed = (euclidean_distance(first.at, second.at) - *pressed).abs();
                *claimed |= changed > self.slop;
            }
            Phase::Started { first, second, .. } => {
                contact(first, second, pointer).at = sample.at();
            }
        }
    }

    fn win(&mut self, pointer: PointerId) -> Vec<Effect> {
        match &mut self.phase {
            Phase::One(first) => first.won = true,
            Phase::Two {
                first,
                second,
                claimed,
                ..
            } => {
                contact(first, second, pointer).won = true;
                let start = euclidean_distance(first.at, second.at);
                if first.won && second.won && start > Dp::ZERO {
                    let (first, second) = (*first, *second);
                    self.phase = Phase::Started {
                        first,
                        second,
                        start,
                    };
                    return self.report(DragPhase::Started);
                }
                let other = if first.pointer == pointer {
                    *second
                } else {
                    *first
                };
                if *claimed && !other.won {
                    return vec![Box::new(move |view: &mut View| {
                        view.step_peer(other.pointer);
                    })];
                }
            }
            Phase::Started { .. } => {}
        }
        Vec::new()
    }

    fn report(&self, phase: DragPhase) -> Vec<Effect> {
        let Phase::Started {
            first,
            second,
            start,
        } = self.phase
        else {
            return Vec::new();
        };
        let node = self.node;
        let scale = euclidean_distance(first.at, second.at) / start;
        let focal = midpoint(first.at, second.at);
        vec![Box::new(move |view: &mut View| {
            view.send_transform(node, focal, scale, phase);
        })]
    }

    fn leave(&mut self, pointer: PointerId, phase: DragPhase) -> Vec<Effect> {
        let report = self.report(phase);
        if let Phase::Two { first, second, .. } | Phase::Started { first, second, .. } = self.phase
        {
            self.phase = Phase::One(if first.pointer == pointer {
                second
            } else {
                first
            });
        }
        report
    }
}

fn contact<'a>(
    first: &'a mut Contact,
    second: &'a mut Contact,
    pointer: PointerId,
) -> &'a mut Contact {
    if first.pointer == pointer {
        first
    } else {
        second
    }
}

impl View {
    fn send_transform(&mut self, node: NodeId, focal: Point, scale: f64, phase: DragPhase) {
        if let Some((callback, rect)) = self.handler(node, Gestures::transform) {
            callback.borrow_mut()(TransformInput {
                focal: relative(focal, rect),
                scale,
                phase,
            });
        }
    }
}
