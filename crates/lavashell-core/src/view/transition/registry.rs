// SPDX-License-Identifier: GPL-3.0-only
use super::{Departure, Exit, MaterialSteps, Transition, Visual};
use crate::animation::{Animation, AnimationError, Motion};
use crate::ui::NodeId;
use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};

enum Lifecycle {
    InTree,
    Leaving {
        departure: Departure,
        exit: Box<Exit>,
    },
}

struct TransitionState {
    animation: Animation<Visual>,
    transition: Transition,
    lifecycle: Lifecycle,
    material_steps: MaterialSteps,
}

impl TransitionState {
    fn is_leaving(&self) -> bool {
        matches!(self.lifecycle, Lifecycle::Leaving { .. })
    }

    fn departure(&self) -> Option<&Departure> {
        match &self.lifecycle {
            Lifecycle::Leaving { departure, .. } => Some(departure),
            Lifecycle::InTree => None,
        }
    }

    fn departure_mut(&mut self) -> Option<&mut Departure> {
        match &mut self.lifecycle {
            Lifecycle::Leaving { departure, .. } => Some(departure),
            Lifecycle::InTree => None,
        }
    }

    fn take_departure(&mut self) -> Option<Departure> {
        match std::mem::replace(&mut self.lifecycle, Lifecycle::InTree) {
            Lifecycle::Leaving { departure, .. } => Some(departure),
            Lifecycle::InTree => None,
        }
    }

    fn exit_ended(&self) -> bool {
        self.is_leaving() && !self.animation.is_running()
    }

    fn animate_to(&self, target: Visual) -> Result<(), AnimationError> {
        match &self.lifecycle {
            Lifecycle::InTree => self.animation.animate_to(target, self.transition.spec),
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
}

#[derive(Default)]
pub(in crate::view) struct Transitions {
    states: BTreeMap<NodeId, TransitionState>,
}

impl Transitions {
    pub fn is_leaving(&self, id: NodeId) -> bool {
        self.states
            .get(&id)
            .is_some_and(TransitionState::is_leaving)
    }

    pub fn refresh(
        &mut self,
        motion: &Motion,
        id: NodeId,
        transition: &Transition,
        target: Visual,
        steps: MaterialSteps,
    ) -> Result<(&Animation<Visual>, MaterialSteps), AnimationError> {
        let state = match self.states.entry(id) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => {
                let animation = motion.animation(Visual {
                    presence: transition.enter.unwrap_or_default(),
                    ..target.clone()
                })?;
                entry.insert(TransitionState {
                    animation,
                    transition: *transition,
                    lifecycle: Lifecycle::InTree,
                    material_steps: MaterialSteps::default(),
                })
            }
        };
        state.transition = *transition;
        state.material_steps = steps.or(state.material_steps);
        state.animate_to(target)?;
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
