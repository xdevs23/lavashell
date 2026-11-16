// SPDX-License-Identifier: GPL-3.0-only
use super::{Departure, Exit, MaterialSteps, Transition, Visual};
use crate::animation::{Animation, AnimationError, Motion};
use crate::scene::tree::Presence;
use crate::ui::NodeId;
use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};

enum Lifecycle {
    Arriving,
    Resting,
    Leaving {
        departure: Departure,
        exit: Box<Exit>,
    },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ParentChange {
    Kept,
    Changed,
}

struct TransitionState {
    animation: Animation<Visual>,
    transition: Transition,
    lifecycle: Lifecycle,
    material_steps: MaterialSteps,
    parent: Option<NodeId>,
}

impl TransitionState {
    fn is_leaving(&self) -> bool {
        matches!(self.lifecycle, Lifecycle::Leaving { .. })
    }

    fn departure(&self) -> Option<&Departure> {
        match &self.lifecycle {
            Lifecycle::Leaving { departure, .. } => Some(departure),
            Lifecycle::Arriving | Lifecycle::Resting => None,
        }
    }

    fn departure_mut(&mut self) -> Option<&mut Departure> {
        match &mut self.lifecycle {
            Lifecycle::Leaving { departure, .. } => Some(departure),
            Lifecycle::Arriving | Lifecycle::Resting => None,
        }
    }

    fn take_departure(&mut self) -> Option<Departure> {
        if !self.is_leaving() {
            return None;
        }
        let Lifecycle::Leaving { departure, .. } =
            std::mem::replace(&mut self.lifecycle, Lifecycle::Arriving)
        else {
            return None;
        };
        Some(departure)
    }

    fn exit_ended(&self) -> bool {
        self.is_leaving() && !self.animation.is_running()
    }

    fn animate_to(&self, target: Visual) -> Result<(), AnimationError> {
        match &self.lifecycle {
            Lifecycle::Arriving | Lifecycle::Resting => {
                self.animation.animate_to(target, self.transition.spec)
            }
            Lifecycle::Leaving { exit, .. } => self.animation.animate_to(
                Visual {
                    presence: exit.presence,
                    ..target
                },
                exit.spec,
            ),
        }
    }

    fn leave(&mut self, departure: Departure, exit: Exit) -> Result<(), AnimationError> {
        departure.run_exit_actions()?;
        self.lifecycle = Lifecycle::Leaving {
            departure,
            exit: Box::new(exit),
        };
        self.animate_to(self.animation.target())
    }

    fn reparent(&mut self, parent: Option<NodeId>) -> ParentChange {
        if self.parent == parent {
            return ParentChange::Kept;
        }
        self.parent = parent;
        if matches!(self.lifecycle, Lifecycle::Resting) {
            self.lifecycle = Lifecycle::Arriving;
        }
        ParentChange::Changed
    }

    fn follow(&self, target: Visual, parent: ParentChange) -> Result<(), AnimationError> {
        let instant_at_rest = self.transition.instant_at_rest;
        match self.lifecycle {
            Lifecycle::Resting if instant_at_rest => self.animation.snap(target),
            Lifecycle::Arriving
                if instant_at_rest && parent == ParentChange::Kept && self.moves_only(&target) =>
            {
                (self.animation).shift_or_animate_to(target, self.transition.spec)
            }
            _ => self.animate_to(target),
        }
    }

    fn moves_only(&self, target: &Visual) -> bool {
        let end = self.animation.target();
        end.rect != target.rect
            && Visual {
                rect: target.rect,
                ..end
            } == *target
    }

    fn settle(&mut self) {
        if matches!(self.lifecycle, Lifecycle::Arriving) && !self.animation.is_running() {
            self.lifecycle = Lifecycle::Resting;
        }
    }

    fn paints_above(&self) -> bool {
        self.transition.instant_at_rest && matches!(self.lifecycle, Lifecycle::Arriving)
    }
}

#[derive(Clone, Copy, Default)]
enum Enters {
    #[default]
    Play,
    SkipUntilFirstFrameEnds,
}

impl Enters {
    fn presence(self, transition: &Transition) -> Presence {
        match self {
            Self::Play => transition.enter.unwrap_or_default(),
            Self::SkipUntilFirstFrameEnds => Presence::default(),
        }
    }
}

#[derive(Default)]
pub(in crate::view) struct Transitions {
    states: BTreeMap<NodeId, TransitionState>,
    enters: Enters,
}

impl Transitions {
    pub fn skipping_first_frame_enters() -> Self {
        Self {
            enters: Enters::SkipUntilFirstFrameEnds,
            ..Self::default()
        }
    }

    pub fn is_leaving(&self, id: NodeId) -> bool {
        self.states
            .get(&id)
            .is_some_and(TransitionState::is_leaving)
    }

    pub fn paints_above(&self, id: NodeId) -> bool {
        (self.states.get(&id)).is_some_and(TransitionState::paints_above)
    }

    pub fn settle_arrivals(&mut self) -> BTreeSet<NodeId> {
        (self.states.iter_mut())
            .filter_map(|(id, state)| {
                let painted_above = state.paints_above();
                state.settle();
                (painted_above && !state.paints_above()).then_some(*id)
            })
            .collect()
    }

    pub fn end_first_frame(&mut self) {
        self.enters = Enters::Play;
    }

    pub fn refresh(
        &mut self,
        motion: &Motion,
        id: NodeId,
        parent: Option<NodeId>,
        transition: &Transition,
        target: Visual,
        steps: MaterialSteps,
    ) -> Result<(&Animation<Visual>, MaterialSteps), AnimationError> {
        let state = match self.states.entry(id) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => {
                let animation = motion.animation(Visual {
                    presence: self.enters.presence(transition),
                    ..target.clone()
                })?;
                entry.insert(TransitionState {
                    animation,
                    transition: *transition,
                    lifecycle: Lifecycle::Arriving,
                    material_steps: MaterialSteps::default(),
                    parent,
                })
            }
        };
        let parent_change = state.reparent(parent);
        state.transition = *transition;
        state.material_steps = steps.or(state.material_steps);
        state.follow(target, parent_change)?;
        state.settle();
        Ok((&state.animation, state.material_steps))
    }

    pub fn return_to_tree(&mut self, present: &BTreeSet<NodeId>) -> Vec<Departure> {
        (self.states.iter_mut())
            .filter(|(id, _)| present.contains(id))
            .filter_map(|(_, state)| state.take_departure())
            .collect()
    }

    pub fn exits_on_removal(&self, id: NodeId) -> bool {
        (self.states.get(&id)).is_some_and(|state| !state.is_leaving() && state.transition.exits())
    }

    pub fn leave(&mut self, id: NodeId, departure: Departure) -> Result<(), AnimationError> {
        if let Some(state) = self.states.get_mut(&id)
            && !state.is_leaving()
            && let Some(exit) = state.transition.exit
        {
            state.leave(departure, exit)?;
        }
        Ok(())
    }

    pub fn departure(&mut self, id: NodeId) -> Option<&mut Departure> {
        self.states.get_mut(&id)?.departure_mut()
    }

    pub fn departures(&self) -> impl Iterator<Item = &Departure> {
        self.states.values().filter_map(TransitionState::departure)
    }

    pub fn subtree(&self, id: NodeId) -> Vec<NodeId> {
        let departure = self.states.get(&id).and_then(TransitionState::departure);
        departure.map_or_else(Vec::new, |departure| departure.ids().collect())
    }

    pub fn exits_ended(&self) -> bool {
        self.states.values().any(TransitionState::exit_ended)
    }

    pub fn ended_exits(&self) -> Vec<NodeId> {
        (self.states.iter())
            .filter(|(_, state)| state.exit_ended())
            .map(|(id, _)| *id)
            .collect()
    }

    pub fn remove(&mut self, id: NodeId) {
        let _transition_of_a_removed_node = self.states.remove(&id);
    }
}

#[cfg(test)]
mod tests;
