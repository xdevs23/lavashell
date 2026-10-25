// SPDX-License-Identifier: GPL-3.0-only
use super::{
    Animation, BTreeMap, BTreeSet, Departure, MaterialSteps, NodeId, Presence, Transition, Visual,
};
use crate::animation::{AnimationError, Motion};
use std::collections::btree_map::Entry;

enum Lifecycle {
    InTree,
    Leaving {
        departure: Departure,
        exit: Presence,
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

    /// Animates the node to `target`. While the node leaves, the exit presence replaces the
    /// presence of `target`.
    fn animate_to(&self, mut target: Visual) -> Result<(), AnimationError> {
        if let Lifecycle::Leaving { exit, .. } = self.lifecycle {
            target.presence = exit;
        }
        self.animation.animate_to(target, self.transition.spec)
    }

    fn leave(&mut self, departure: Departure, exit: Presence) -> Result<(), AnimationError> {
        departure.run_exit_actions()?;
        self.lifecycle = Lifecycle::Leaving { departure, exit };
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

    /// Animates the node `id` to `target` with `transition`. A node without transition state first
    /// gets an animation on the clock of `motion` that starts at the enter presence of
    /// `transition`. Records `steps` as the material steps of the node and returns them, filled in
    /// with the previously recorded step for each material that `steps` lacks. Fails when a value
    /// of `target` is not finite.
    pub fn refresh(
        &mut self,
        motion: &Motion,
        id: NodeId,
        transition: Transition,
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
                    transition,
                    lifecycle: Lifecycle::InTree,
                    material_steps: MaterialSteps::default(),
                })
            }
        };
        state.transition = transition;
        state.material_steps = steps.or(state.material_steps);
        state.animate_to(target)?;
        Ok((&state.animation, state.material_steps))
    }

    /// Ends the exit of every leaving node in `present`, because those nodes are composed again.
    /// Returns the departures that those nodes had while they were leaving.
    pub fn return_to_tree(&mut self, present: &BTreeSet<NodeId>) -> Vec<Departure> {
        (self.states.iter_mut())
            .filter(|(id, _)| present.contains(id))
            .filter_map(|(_, state)| state.take_departure())
            .collect()
    }

    /// Whether the node `id` starts an exit when it is removed: it is in the tree and has an exit.
    pub fn exits_on_removal(&self, id: NodeId) -> bool {
        (self.states.get(&id)).is_some_and(|state| !state.is_leaving() && state.transition.exits())
    }

    /// Starts the exit of the node `id` with `departure`, when the node exits on removal. Fails
    /// when an animation of the node cannot move to its exit value.
    pub fn leave(&mut self, id: NodeId, departure: Departure) -> Result<(), AnimationError> {
        if let Some(state) = self.states.get_mut(&id)
            && !state.is_leaving()
            && let Some(exit) = state.transition.exit
        {
            state.leave(departure, exit)?;
        }
        Ok(())
    }

    /// The departure of the node `id` while the node leaves, until `remove` drops the state of the
    /// node.
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
