// SPDX-License-Identifier: GPL-3.0-only
mod arena;
mod drag;
mod pinch;
mod raw;
mod scroll;
mod select;
mod tap;

use super::pointers::PointerSample;
use super::{
    Axis, Dp, DragInput, DragPhase, FocusSource, Hit, NodeId, Point, PointerId, PointerInput,
    PointerPhase, Rect, TransformInput, View, index_of,
};
pub(super) use arena::{Arena, Outcome};
use std::any::Any;

/// A change to the view returned by a recognizer, run by `View::apply_outcome` after the step of the arena.
pub(super) type Effect = Box<dyn FnOnce(&mut View)>;

/// Something that happened to a pressed pointer, passed to the recognizers in the arena of that pointer.
#[derive(Clone, Copy)]
pub(super) enum Event {
    Press(PointerSample),
    Move(PointerSample),
    /// The pointer was released at `sample`. `over` is the node hit at that point, if any.
    Release {
        sample: PointerSample,
        over: Option<NodeId>,
    },
    /// A recognizer that is also in the arena of another pointer changed, such as a pinch that follows two pointers.
    Peer,
}

/// The result of `Recognizer::offer` for a pointer that no recognizer has won yet.
pub(super) enum Verdict {
    Wait,
    Accept,
    /// Accepts the pointer for every recognizer declared on the same node.
    AcceptWithNode,
    Reject,
}

/// A recognizer of one gesture, declared on a node. For each press, the recognizers on the hit path of the press
/// compete in an arena for the pressed pointer.
pub(super) trait Recognizer: Any {
    fn node(&self) -> NodeId;

    /// Whether the recognizer waits for more events, accepts or rejects the pointer pressed with `press` after
    /// `event`. The arena calls this only while no recognizer has won the pointer.
    fn offer(&mut self, press: PointerSample, event: Event) -> Verdict;

    /// The changes to the view made by the recognizer after `event`, once the recognizer has won the pointer
    /// pressed with `press`.
    fn run(&mut self, press: PointerSample, event: Event) -> Vec<Effect>;

    /// The changes to the view made by the recognizer when it leaves the arena without a release: when it lost the
    /// pointer, when the press on its node ended, or when the pointer was cancelled after the recognizer won it.
    fn cancel(&mut self) -> Vec<Effect> {
        Vec::new()
    }

    /// Whether the recognizer waits as a challenger once a recognizer on its node or inside it wins the pointer. A
    /// challenger takes the pointer from the winner once it accepts the pointer.
    fn challenge(&mut self) -> bool {
        false
    }

    /// Whether the node of the recognizer still declares it after the node was composed again.
    fn declared(&self, _view: &View) -> bool {
        true
    }
}

/// A kind of recognizer. `declare` creates a recognizer of this kind for a press on a node that declares one.
pub(super) struct Registration {
    precedence: Precedence,
    /// Whether `View::node_recognizers` creates this kind only for a press on a node without any recognizer of the
    /// other kinds. A recognizer of this kind then never competes with those.
    alone: bool,
    declare: fn(&View, &Press) -> Option<Box<dyn Recognizer>>,
}

/// The rank of a kind of recognizer among the recognizers declared on one node. The recognizer of the lowest rank
/// comes first in the arena. It wins the pointer when several recognizers accept the pointer in the same step.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Precedence(u8);

/// The registrations of every kind of recognizer, in any order. `View::node_recognizers` sorts them by precedence.
const REGISTRATIONS: [Registration; 7] = [
    select::REGISTRATION,
    raw::REGISTRATION,
    tap::CLICK,
    drag::REGISTRATION,
    scroll::REGISTRATION,
    pinch::REGISTRATION,
    tap::FOCUS,
];

/// A press with `sample` on the node at `index`, for which that node declares recognizers. `innermost` is true for
/// the innermost node of the hit path. `slop` is how far the pointer moves before a recognizer that waits for
/// movement accepts the pointer.
pub(super) struct Press {
    pub sample: PointerSample,
    pub index: usize,
    pub innermost: bool,
    pub slop: Dp,
}

impl Press {
    fn pointer(&self) -> PointerId {
        self.sample.pointer()
    }

    fn node(&self, view: &View) -> NodeId {
        view.nodes[self.index].id
    }
}

impl View {
    /// The recognizers declared for the press of `sample` on the node `hit`, from the innermost node out. For any
    /// pointer other than a mouse, the list also contains the recognizers on the ancestors of `hit` that have a hit
    /// at the pressed point.
    pub(super) fn press_recognizers(
        &self,
        sample: PointerSample,
        hit: NodeId,
    ) -> Vec<Box<dyn Recognizer>> {
        let mut path = self.hit_path(hit, sample.at());
        if !sample.kind().drags_content() {
            path.truncate(1);
        }
        let slop = self.touch_slop();
        let press = |(depth, index): (usize, usize)| Press {
            sample,
            index,
            innermost: depth == 0,
            slop,
        };
        (path.into_iter().enumerate())
            .flat_map(|step| self.node_recognizers(&press(step)))
            .collect()
    }

    /// The indices of the node `hit` and of each ancestor with a hit at `at`, from the innermost node out.
    fn hit_path(&self, hit: NodeId, at: Point) -> Vec<usize> {
        let Some(mut index) = index_of(&self.nodes, hit) else {
            return Vec::new();
        };
        let mut path = vec![index];
        while let Some(parent) = self.parents.node_of(index) {
            let id = self.nodes[parent].id;
            if self
                .hits
                .iter()
                .any(|hit| hit.id == id && hit.contains(at, &self.moving))
            {
                path.push(parent);
            }
            index = parent;
        }
        path
    }

    /// The recognizers declared by the node of `press`, in the order of their precedence.
    fn node_recognizers(&self, press: &Press) -> Vec<Box<dyn Recognizer>> {
        let mut registrations: Vec<&Registration> = REGISTRATIONS.iter().collect();
        registrations.sort_by_key(|registration| registration.precedence);
        let declared = |alone: bool| -> Vec<_> {
            (registrations.iter())
                .filter(|registration| registration.alone == alone)
                .filter_map(|registration| (registration.declare)(self, press))
                .collect()
        };
        let competing = declared(false);
        if competing.is_empty() {
            declared(true)
        } else {
            competing
        }
    }

    fn pressed<R: Recognizer>(&self) -> impl Iterator<Item = &R> {
        (self.pressed_recognizers())
            .filter_map(|recognizer| (recognizer as &dyn Any).downcast_ref())
    }

    /// Applies the outcome of a step of an arena: moves the focus to the node that won the pointer, then runs the
    /// changes returned by the recognizers.
    pub(super) fn apply_outcome(&mut self, outcome: Outcome) {
        if let Some(node) = outcome.won {
            self.focus_winner(node);
        }
        if !outcome.effects.is_empty() {
            self.redraw = true;
        }
        for effect in outcome.effects {
            effect(self);
        }
    }

    /// Moves the focus to `node`, which won a pointer, when `node` can receive focus, and records the pointer as the
    /// source of the focus. Also reports the press on `node` to the input method sessions.
    fn focus_winner(&mut self, node: NodeId) {
        self.sessions.press(node);
        if self.hits.iter().any(|hit| hit.id == node && hit.focusable) {
            self.focus = Some(node);
            self.record_focus_source(FocusSource::Pointer);
        }
        self.redraw = true;
    }

    pub(super) fn hits_of(&self, node: NodeId) -> impl Iterator<Item = &Hit> {
        self.hits.iter().filter(move |hit| hit.id == node)
    }

    /// The handler returned by `pick` for the first hit of `node` for which `pick` returns one, with the bounds of
    /// that hit as the consumer shows it.
    pub(super) fn handler<T>(
        &self,
        node: NodeId,
        pick: impl Fn(&Hit) -> Option<T>,
    ) -> Option<(T, Rect)> {
        self.hits_of(node)
            .find_map(|hit| Some((pick(hit)?, self.moving.shown(hit.group, hit.rect))))
    }
}

/// The verdict of a recognizer for a movement along `axis`. The recognizer accepts a pointer that moved further
/// than `slop` along `axis`, and at least as far along `axis` as across it. Otherwise the recognizer waits.
fn accept_along(axis: Axis, slop: Dp, press: PointerSample, event: Event) -> Verdict {
    let Event::Move(sample) = event else {
        return Verdict::Wait;
    };
    let along = axis.signed_distance(press.at(), sample.at()).abs();
    let across = axis.cross().signed_distance(press.at(), sample.at()).abs();
    if along > slop && along >= across {
        Verdict::Accept
    } else {
        Verdict::Wait
    }
}

fn relative(at: Point, rect: Rect) -> Point {
    Point::new(at.x - rect.x, at.y - rect.y)
}

fn euclidean_distance(from: Point, to: Point) -> Dp {
    (to.x - from.x).hypot(to.y - from.y)
}

fn midpoint(a: Point, b: Point) -> Point {
    Point::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn each_kind_of_recognizer_has_its_own_precedence() {
        let ranks: BTreeSet<Precedence> = (REGISTRATIONS.iter())
            .map(|registration| registration.precedence)
            .collect();
        assert_eq!(ranks.len(), REGISTRATIONS.len());
    }
}
