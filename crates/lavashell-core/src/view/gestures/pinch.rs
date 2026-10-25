// SPDX-License-Identifier: GPL-3.0-only
use super::{
    Dp, DragPhase, Effect, Event, NodeId, Point, PointerId, PointerSample, Precedence, Press,
    Recognizer, Registration, TransformInput, Verdict, View, euclidean_distance, midpoint,
    relative,
};
use std::cell::RefCell;
use std::rc::Rc;

pub(super) const REGISTRATION: Registration = Registration {
    precedence: Precedence(5),
    alone: false,
    declare: declared,
};

/// A pinch by two pointers pressed on one node, such as two fingers.
struct Pinch {
    node: NodeId,
    slop: Dp,
    phase: Phase,
}

/// One point of a pinch: the pointer, the position of the pointer, and whether the pinch won the pointer in the
/// arena of that pointer.
#[derive(Clone, Copy)]
struct Contact {
    pointer: PointerId,
    at: Point,
    won: bool,
}

enum Phase {
    /// One point is down.
    One(Contact),
    /// Both points are down. `pressed` is the distance between the points at the press of the second point.
    /// `claimed` is true once that distance changed by more than the slop.
    Two {
        first: Contact,
        second: Contact,
        pressed: Dp,
        claimed: bool,
    },
    /// The pinch won both points and started. `start` is the distance between the points at that moment.
    Started {
        first: Contact,
        second: Contact,
        start: Dp,
    },
}

/// The recognizer of one point of a pinch, in the arena of that point. The recognizers of both
/// points share the pinch.
pub(super) struct PinchPoint {
    pinch: Rc<RefCell<Pinch>>,
    pointer: PointerId,
}

/// The recognizer for a pointer other than a mouse, pressed on a node with `on_transform`. The pointer joins a pinch
/// on the node that has only one point, or else starts a new pinch.
fn declared(view: &View, press: &Press) -> Option<Box<dyn Recognizer>> {
    let node = press.node(view);
    let pinches = view.hits_of(node).any(|hit| hit.transform.is_some());
    if !pinches || !press.sample.kind().drags_content() {
        return None;
    }
    let pointer = press.pointer();
    let joined = (view.pressed::<PinchPoint>()).find_map(|point| point.join(node, pointer));
    Some(Box::new(joined.unwrap_or_else(|| {
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
        PinchPoint {
            pinch: Rc::new(RefCell::new(pinch)),
            pointer,
        }
    })))
}

impl PinchPoint {
    /// The recognizer of `pointer` as the second point of this pinch, when the pinch is on `node` and has one point.
    /// `Pinch::follow` adds the second point at the press of `pointer`.
    fn join(&self, node: NodeId, pointer: PointerId) -> Option<Self> {
        let pinch = self.pinch.borrow();
        let alone = matches!(pinch.phase, Phase::One(first) if first.pointer != pointer);
        (alone && pinch.node == node).then(|| Self {
            pinch: self.pinch.clone(),
            pointer,
        })
    }
}

impl Recognizer for PinchPoint {
    fn node(&self) -> NodeId {
        self.pinch.borrow().node
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
}

impl Pinch {
    /// Moves the point of `pointer` to the position sampled by `event`. At the press of a second pointer, adds that
    /// pointer as the second point.
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

    /// Marks the point of `pointer` as won by the pinch. Starts the pinch once the pinch won both points and the
    /// points lie apart. When the distance between the points changed by more than the slop, the returned change
    /// steps the arena of the other point, so that the pinch wins that point too.
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

    /// Removes the point of `pointer` from the pinch. When the pinch had started, returns a report to the handler
    /// with `phase`. The pinch then has only the other point, and a new pointer can join it as the second point.
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
        if let Some((callback, rect)) = self.handler(node, |hit| hit.transform.clone()) {
            callback.borrow_mut()(TransformInput {
                focal: relative(focal, rect),
                scale,
                phase,
            });
        }
    }
}
