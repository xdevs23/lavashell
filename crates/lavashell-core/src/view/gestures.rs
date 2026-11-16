// SPDX-License-Identifier: GPL-3.0-only
mod arena;
mod drag;
mod pinch;
mod raw;
mod scroll;
mod select;
mod tap;

use super::input::InputRouting;
use super::pointers::PointerSample;
use super::{Hit, View};
use crate::geometry::{Axis, Point, Rect};
use crate::ui::{NodeId, PointerId};
use crate::units::Dp;
use crate::view::component::index_of;
use crate::view::interaction::FocusSource;
pub(super) use arena::{Arena, Outcome};

pub(super) type Effect = Box<dyn FnOnce(&mut View)>;

#[derive(Clone, Copy)]
pub(super) enum Event {
    Press(PointerSample),
    Move(PointerSample),
    Release {
        sample: PointerSample,
        over: Option<NodeId>,
    },
    Peer,
}

pub(super) enum Verdict {
    Wait,
    Accept,
    AcceptWithNode,
    Reject,
}

pub(super) trait Recognizer {
    fn node(&self) -> NodeId;

    fn contender(&self) -> Contender;

    fn offer(&mut self, press: PointerSample, event: Event) -> Verdict;

    fn run(&mut self, press: PointerSample, event: Event) -> Vec<Effect>;

    fn cancel(&mut self) -> Vec<Effect> {
        Vec::new()
    }

    fn challenge(&mut self) -> bool {
        false
    }

    fn declared(&self, _view: &View) -> bool {
        true
    }

    fn joined_by(&self, _view: &View, _press: &Press) -> Option<Box<dyn Recognizer>> {
        None
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Contest {
    Competing,
    Fallback,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Contender {
    Select,
    Raw,
    Click,
    Drag,
    Scroll,
    Pinch,
    Focus,
}

impl Contender {
    const ALL: [Self; 7] = [
        Self::Select,
        Self::Raw,
        Self::Click,
        Self::Drag,
        Self::Scroll,
        Self::Pinch,
        Self::Focus,
    ];

    fn rank(self) -> u8 {
        match self {
            Self::Select => 0,
            Self::Raw => 1,
            Self::Click => 2,
            Self::Drag => 3,
            Self::Scroll => 4,
            Self::Pinch => 5,
            Self::Focus => 6,
        }
    }

    fn contest(self) -> Contest {
        match self {
            Self::Select | Self::Raw | Self::Click | Self::Drag | Self::Scroll | Self::Pinch => {
                Contest::Competing
            }
            Self::Focus => Contest::Fallback,
        }
    }

    fn declare(self, view: &View, press: &Press) -> Option<Box<dyn Recognizer>> {
        let declared = match self {
            Self::Select => select::declared(view, press),
            Self::Raw => raw::declared(view, press),
            Self::Click => tap::clicking(view, press),
            Self::Drag => drag::declared(view, press),
            Self::Scroll => scroll::declared(view, press),
            Self::Pinch => pinch::declared(view, press),
            Self::Focus => tap::focusing(view, press),
        }?;
        let joined = (view.routing.pressed_recognizers())
            .filter(|pressed| pressed.contender() == self)
            .find_map(|pressed| pressed.joined_by(view, press));
        Some(joined.unwrap_or(declared))
    }
}

pub(super) struct Press {
    pub sample: PointerSample,
    pub index: usize,
    pub node: NodeId,
    pub innermost: bool,
    pub slop: Dp,
}

impl Press {
    fn pointer(&self) -> PointerId {
        self.sample.pointer()
    }
}

impl View {
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
            node: self.nodes[index].id,
            innermost: depth == 0,
            slop,
        };
        (path.into_iter().enumerate())
            .flat_map(|step| self.node_recognizers(&press(step)))
            .collect()
    }

    fn hit_path(&self, hit: NodeId, at: Point) -> Vec<usize> {
        let Some(mut index) = index_of(&self.nodes, hit) else {
            return Vec::new();
        };
        let mut path = vec![index];
        while let Some(parent) = self.parents.node_of(index) {
            let id = self.nodes[parent].id;
            if self
                .routing
                .hits
                .iter()
                .any(|hit| hit.id == id && hit.contains(at, &self.routing.moving))
            {
                path.push(parent);
            }
            index = parent;
        }
        path
    }

    fn node_recognizers(&self, press: &Press) -> Vec<Box<dyn Recognizer>> {
        let mut contenders = Contender::ALL;
        contenders.sort_by_key(|contender| contender.rank());
        let declared = |contest: Contest| -> Vec<_> {
            (contenders.iter())
                .filter(|contender| contender.contest() == contest)
                .filter_map(|contender| contender.declare(self, press))
                .collect()
        };
        let competing = declared(Contest::Competing);
        if competing.is_empty() {
            declared(Contest::Fallback)
        } else {
            competing
        }
    }

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

    fn focus_winner(&mut self, node: NodeId) {
        self.editing.sessions.press(node);
        if self
            .routing
            .hits
            .iter()
            .any(|hit| hit.id == node && hit.focusable)
        {
            self.routing.focus = Some(node);
            self.record_focus_source(FocusSource::Pointer);
        }
        self.redraw = true;
    }

    pub(super) fn handler<T>(
        &self,
        node: NodeId,
        pick: impl Fn(&Hit) -> Option<T>,
    ) -> Option<(T, Rect)> {
        self.routing
            .hits_of(node)
            .find_map(|hit| Some((pick(hit)?, self.routing.moving.shown(hit.group, hit.rect))))
    }
}

impl InputRouting {
    pub(super) fn hits_of(&self, node: NodeId) -> impl Iterator<Item = &Hit> {
        self.hits.iter().filter(move |hit| hit.id == node)
    }
}

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
    fn each_contender_has_its_own_rank() {
        let ranks: BTreeSet<u8> = Contender::ALL
            .iter()
            .map(|contender| contender.rank())
            .collect();
        assert_eq!(ranks.len(), Contender::ALL.len());
    }
}
