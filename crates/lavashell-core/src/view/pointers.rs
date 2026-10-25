// SPDX-License-Identifier: GPL-3.0-only
use super::gestures::{Arena, Event};
use super::{Hit, NodeId, Point, PointerId, PointerKind, TouchSample, View, gestures};

/// The node under a pointer and, while the pointer is down, the recognizers that compete for the
/// press of that pointer.
#[derive(Default)]
pub(super) struct Pointer {
    pub over: Option<NodeId>,
    arena: Option<Arena>,
}

impl Pointer {
    /// The node pressed by the pointer: the node that won it, or else the innermost node that can
    /// still win it.
    pub fn capture(&self) -> Option<NodeId> {
        self.arena.as_ref()?.capture()
    }

    /// The node that won the press of the pointer, and every node that can still win it.
    pub fn nodes(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.arena.iter().flat_map(Arena::nodes)
    }
}

/// A position of a pointer reported by the host: of the primary pointer, or of a touch point.
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

    /// The device that moves the pointer. The primary pointer works as a mouse.
    pub(super) fn kind(self) -> PointerKind {
        match self {
            Self::Primary(_) => PointerKind::Mouse,
            Self::Touch(sample) => sample.kind,
        }
    }
}

impl View {
    /// The pointer used by hosts that pass only a position.
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

    /// Whether a pointer that pressed `id` is over it.
    pub(super) fn is_pressed(&self, id: NodeId) -> bool {
        (self.pointers.values())
            .any(|pointer| pointer.capture() == Some(id) && pointer.over == Some(id))
    }

    pub(super) fn end_captures(&mut self, ended: impl Fn(NodeId) -> bool) {
        let pointers: Vec<_> = self.pointers.keys().copied().collect();
        for pointer in pointers {
            if let Some(arena) = self.arena(pointer) {
                let outcome = arena.end(&ended);
                self.apply_outcome(outcome);
            }
        }
    }

    pub(super) fn cancel_pointers(&mut self) {
        self.end_captures(|_| true);
        self.pointers.clear();
    }

    /// Cancels the press of `pointer` and removes the pointer.
    pub(super) fn cancel_pointer(&mut self, pointer: PointerId) {
        self.cancel_press(pointer);
        let _state_of_a_cancelled_pointer = self.pointers.remove(&pointer);
        self.redraw = true;
    }

    pub(super) fn hit_at(&self, at: Point) -> Option<&Hit> {
        self.hits
            .iter()
            .rev()
            .find(|hit| hit.contains(at, &self.moving))
    }

    /// Ends the press of `pointer` without a release.
    fn cancel_press(&mut self, pointer: PointerId) {
        let arena = (self.pointers.get_mut(&pointer)).and_then(|state| state.arena.take());
        if let Some(mut arena) = arena {
            let outcome = arena.cancel();
            self.apply_outcome(outcome);
        }
    }

    fn arena(&mut self, pointer: PointerId) -> Option<&mut Arena> {
        self.pointers.get_mut(&pointer)?.arena.as_mut()
    }

    pub(super) fn pressed_recognizers(&self) -> impl Iterator<Item = &dyn gestures::Recognizer> {
        (self.pointers.values())
            .filter_map(|pointer| pointer.arena.as_ref())
            .flat_map(Arena::recognizers)
    }

    fn step_arena(&mut self, pointer: PointerId, event: Event) {
        if let Some(arena) = self.arena(pointer) {
            let outcome = arena.step(event);
            self.apply_outcome(outcome);
        }
    }

    /// Steps the arena of `pointer` after a change of a recognizer that this arena shares with the arena of another
    /// pointer.
    pub(super) fn step_peer(&mut self, pointer: PointerId) {
        self.step_arena(pointer, Event::Peer);
    }

    /// Cancels an earlier press of the pointer of `sample`, and opens a new arena for this press with the recognizers
    /// on the hit path. A press outside every hit removes the focus.
    pub(super) fn press(&mut self, sample: PointerSample) {
        let pointer = sample.pointer();
        self.cancel_press(pointer);
        let hit = self.hit_at(sample.at()).map(|hit| hit.id);
        let state = Pointer {
            over: hit,
            arena: None,
        };
        let _state_of_the_previous_press = self.pointers.insert(pointer, state);
        self.redraw = true;
        let Some(hit) = hit else {
            self.focus = None;
            return;
        };
        let recognizers = self.press_recognizers(sample, hit);
        let (arena, outcome) = Arena::open(sample, recognizers);
        if let Some(state) = self.pointers.get_mut(&pointer) {
            state.arena = Some(arena);
        }
        self.apply_outcome(outcome);
    }

    /// Moves the pointer of `sample`. A touch point that is not down does not move.
    pub(super) fn move_pointer(&mut self, sample: PointerSample) {
        let pointer = sample.pointer();
        if !pointer.hovers() && !self.pointers.contains_key(&pointer) {
            return;
        }
        let over = self.hit_at(sample.at()).map(|hit| hit.id);
        let state = self.pointers.entry(pointer).or_default();
        if state.over != over {
            state.over = over;
            self.redraw = true;
        }
        self.step_arena(pointer, Event::Move(sample));
    }

    pub(super) fn release(&mut self, sample: PointerSample) {
        let pointer = sample.pointer();
        let over = self.hit_at(sample.at()).map(|hit| hit.id);
        self.step_arena(pointer, Event::Release { sample, over });
        if !pointer.hovers() {
            let _state_of_a_released_pointer = self.pointers.remove(&pointer);
        } else if let Some(state) = self.pointers.get_mut(&pointer) {
            state.arena = None;
        }
        self.redraw = true;
    }
}
