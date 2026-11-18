// SPDX-License-Identifier: GPL-3.0-only
use super::gestures::{Arena, Event};
use super::input::InputRouting;
use super::{Hit, TouchSample, View, gestures};
use crate::geometry::Point;
use crate::ui::{NodeId, PointerId, PointerKind};
use crate::window::WindowRegion;

#[derive(Default)]
pub(super) struct Pointer {
    pub over: Option<NodeId>,
    arena: Option<Arena>,
}

impl Pointer {
    pub fn capture(&self) -> Option<NodeId> {
        self.arena.as_ref()?.capture()
    }

    pub fn nodes(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.arena.iter().flat_map(Arena::nodes)
    }
}

#[derive(Clone, Copy)]
pub(super) enum PointerSample {
    Primary(Point),
    Touch(TouchSample),
}

impl PointerSample {
    pub(super) fn pointer(self) -> PointerId {
        match self {
            Self::Primary(_) => PointerId::Primary,
            Self::Touch(sample) => PointerId::Touch(sample.id),
        }
    }

    pub(super) fn at(self) -> Point {
        match self {
            Self::Primary(at) | Self::Touch(TouchSample { at, .. }) => at,
        }
    }

    pub(super) fn kind(self) -> PointerKind {
        match self {
            Self::Primary(_) => PointerKind::Mouse,
            Self::Touch(sample) => sample.kind,
        }
    }
}

impl View {
    pub(super) fn end_captures(&mut self, ended: impl Fn(NodeId) -> bool) {
        let pointers: Vec<_> = self.routing.pointers.keys().copied().collect();
        for pointer in pointers {
            if let Some(arena) = self.routing.arena(pointer) {
                let outcome = arena.end(&ended);
                self.apply_outcome(outcome);
            }
        }
    }

    pub(super) fn withdraw_challengers(&mut self) {
        let pointers: Vec<_> = self.routing.pointers.keys().copied().collect();
        for pointer in pointers {
            let arena =
                (self.routing.pointers.get_mut(&pointer)).and_then(|state| state.arena.take());
            let Some(mut arena) = arena else {
                continue;
            };
            let outcome = arena.withdraw(|challenger| !challenger.declared(self));
            if let Some(state) = self.routing.pointers.get_mut(&pointer) {
                state.arena = Some(arena);
            }
            self.apply_outcome(outcome);
        }
    }

    pub(super) fn cancel_pointers(&mut self) {
        self.end_captures(|_| true);
        self.routing.pointers.clear();
    }

    pub(super) fn cancel_pointer(&mut self, pointer: PointerId) {
        self.cancel_press(pointer);
        let _state_of_a_cancelled_pointer = self.routing.pointers.remove(&pointer);
        self.redraw = true;
    }

    fn cancel_press(&mut self, pointer: PointerId) {
        let arena = (self.routing.pointers.get_mut(&pointer)).and_then(|state| state.arena.take());
        if let Some(mut arena) = arena {
            let outcome = arena.cancel();
            self.apply_outcome(outcome);
        }
    }

    fn step_arena(&mut self, pointer: PointerId, event: Event) {
        if let Some(arena) = self.routing.arena(pointer) {
            let outcome = arena.step(event);
            self.apply_outcome(outcome);
        }
    }

    pub(super) fn step_peer(&mut self, pointer: PointerId) {
        self.step_arena(pointer, Event::Peer);
    }

    pub(super) fn press(&mut self, sample: PointerSample) {
        let pointer = sample.pointer();
        self.cancel_press(pointer);
        let hit = self.routing.hit_at(sample.at()).map(|hit| hit.id);
        let state = Pointer {
            over: hit,
            arena: None,
        };
        let _state_of_the_previous_press = self.routing.pointers.insert(pointer, state);
        self.redraw = true;
        let Some(hit) = hit else {
            self.routing.focus.clear();
            return;
        };
        let recognizers = self.press_recognizers(sample, hit);
        let (arena, outcome) = Arena::open(sample, recognizers);
        if let Some(state) = self.routing.pointers.get_mut(&pointer) {
            state.arena = Some(arena);
        }
        self.apply_outcome(outcome);
    }

    pub(super) fn move_pointer(&mut self, sample: PointerSample) {
        let pointer = sample.pointer();
        if !pointer.hovers() && !self.routing.pointers.contains_key(&pointer) {
            return;
        }
        let over = self.routing.hit_at(sample.at()).map(|hit| hit.id);
        let state = self.routing.pointers.entry(pointer).or_default();
        if state.over != over {
            state.over = over;
            self.redraw = true;
        }
        self.step_arena(pointer, Event::Move(sample));
    }

    pub(super) fn release(&mut self, sample: PointerSample) {
        let pointer = sample.pointer();
        let over = self.routing.hit_at(sample.at()).map(|hit| hit.id);
        self.step_arena(pointer, Event::Release { sample, over });
        if !pointer.hovers() {
            let _state_of_a_released_pointer = self.routing.pointers.remove(&pointer);
        } else if let Some(state) = self.routing.pointers.get_mut(&pointer) {
            state.arena = None;
        }
        self.redraw = true;
    }
}

impl InputRouting {
    pub(super) fn primary(&self) -> Option<&Pointer> {
        self.pointers.get(&PointerId::Primary)
    }
    pub(super) fn is_captured(&self, id: NodeId) -> bool {
        self.pointers
            .values()
            .any(|pointer| pointer.capture() == Some(id))
    }
    pub(super) fn is_hovered(&self, id: NodeId) -> bool {
        self.pointers
            .values()
            .any(|pointer| pointer.over == Some(id))
    }
    pub(super) fn is_pressed(&self, id: NodeId) -> bool {
        (self.pointers.values())
            .any(|pointer| pointer.capture() == Some(id) && pointer.over == Some(id))
    }
    pub(super) fn hit_at(&self, at: Point) -> Option<&Hit> {
        self.hits
            .iter()
            .rev()
            .find(|hit| hit.contains(at, &self.moving))
    }
    pub(super) fn window_region_at(&self, at: Point) -> Option<WindowRegion> {
        let topmost = (self.window_regions.iter().rev())
            .find(|region| region.area.contains(at, &self.moving))?;
        let covered = (self.hits.iter().skip(topmost.input_hits_below))
            .any(|hit| hit.contains(at, &self.moving));
        (!covered).then_some(topmost.region)
    }
    fn arena(&mut self, pointer: PointerId) -> Option<&mut Arena> {
        self.pointers.get_mut(&pointer)?.arena.as_mut()
    }
    pub(super) fn pressed_recognizers(&self) -> impl Iterator<Item = &dyn gestures::Recognizer> {
        (self.pointers.values())
            .filter_map(|pointer| pointer.arena.as_ref())
            .flat_map(Arena::recognizers)
    }
}
