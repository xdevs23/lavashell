// SPDX-License-Identifier: GPL-3.0-only
use super::{Effect, Event, NodeId, PointerSample, Recognizer, Verdict};

/// The recognizers that compete for one pressed pointer, in the order of the hit path from the
/// innermost node out, and the node that won the pointer. Once a node won, `members` contains only
/// the recognizers that won.
pub(in crate::view) struct Arena {
    /// The sample of the press, from which recognizers measure the movement of the pointer.
    press: PointerSample,
    members: Vec<Box<dyn Recognizer>>,
    winner: Option<NodeId>,
}

/// The result of one step of an arena: the node that won the pointer in the step, and the changes to the view
/// returned by the recognizers.
#[derive(Default)]
pub(in crate::view) struct Outcome {
    /// The node that won the pointer in the step.
    pub won: Option<NodeId>,
    pub effects: Vec<Effect>,
}

impl Arena {
    /// The arena of a pointer pressed with `press`, and the outcome of the press.
    pub fn open(press: PointerSample, members: Vec<Box<dyn Recognizer>>) -> (Self, Outcome) {
        let mut arena = Self {
            press,
            members,
            winner: None,
        };
        let outcome = arena.step(Event::Press(press));
        (arena, outcome)
    }

    /// The node that won the pointer, or else the innermost node that can still win it.
    pub fn capture(&self) -> Option<NodeId> {
        Some(self.members.first()?.node())
    }

    /// The node that won the pointer and every node that can still win it.
    pub fn nodes(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.members.iter().map(|member| member.node())
    }

    pub fn recognizers(&self) -> impl Iterator<Item = &dyn Recognizer> {
        self.members.iter().map(Box::as_ref)
    }

    /// Passes `event` to the recognizers. While no recognizer has won the pointer, the first recognizer that accepts
    /// the pointer wins it. The pointer also goes to the only recognizer left after the others rejected it, and at a
    /// release to the innermost recognizer left. Then the recognizers that won the pointer run with `event`.
    pub fn step(&mut self, event: Event) -> Outcome {
        let mut outcome = Outcome::default();
        let press = self.press;
        if self.winner.is_none() {
            let verdicts: Vec<_> = (self.members.iter_mut())
                .map(|member| member.offer(press, event))
                .collect();
            let accepted = verdicts
                .iter()
                .enumerate()
                .find_map(|(index, verdict)| match verdict {
                    Verdict::Accept => Some((index, false)),
                    Verdict::AcceptWithNode => Some((index, true)),
                    Verdict::Wait | Verdict::Reject => None,
                });
            if let Some((index, with_node)) = accepted {
                self.decide(index, with_node, &mut outcome);
            } else {
                let mut verdicts = verdicts.into_iter();
                let rejected =
                    |_: &dyn Recognizer| matches!(verdicts.next(), Some(Verdict::Reject));
                self.remove(rejected, &mut outcome);
                let swept = matches!(event, Event::Release { .. }) && !self.members.is_empty();
                if self.members.len() == 1 || swept {
                    self.decide(0, false, &mut outcome);
                }
            }
        }
        if self.winner.is_some() {
            for member in &mut self.members {
                outcome.effects.extend(member.run(press, event));
            }
        }
        outcome
    }

    /// Removes the recognizers declared on the nodes for which `ended` is true. When the node that won the pointer
    /// ends, every recognizer leaves the arena, and no other node gets the pointer before the release.
    pub fn end(&mut self, ended: impl Fn(NodeId) -> bool) -> Outcome {
        if self.winner.is_some_and(&ended) {
            return self.cancel();
        }
        let mut outcome = Outcome::default();
        self.remove(|member| ended(member.node()), &mut outcome);
        if self.winner.is_none() && self.members.len() == 1 {
            self.decide(0, false, &mut outcome);
        }
        outcome
    }

    /// Removes every recognizer from the arena without a release.
    pub fn cancel(&mut self) -> Outcome {
        let mut outcome = Outcome::default();
        self.winner = None;
        self.remove(|_| true, &mut outcome);
        outcome
    }

    /// Removes the recognizers for which `removed` is true, and adds to `outcome` the changes returned by
    /// `Recognizer::cancel` of each removed recognizer.
    fn remove(&mut self, mut removed: impl FnMut(&dyn Recognizer) -> bool, outcome: &mut Outcome) {
        self.members.retain_mut(|member| {
            let leaves = removed(member.as_ref());
            if leaves {
                outcome.effects.extend(member.cancel());
            }
            !leaves
        });
    }

    /// Gives the pointer to the recognizer at `index`, and to the other recognizers declared on the
    /// same node when `with_node` is true.
    fn decide(&mut self, index: usize, with_node: bool, outcome: &mut Outcome) {
        let node = self.members[index].node();
        let mut position = 0..;
        let lost = |member: &dyn Recognizer| {
            position.next() != Some(index) && !(with_node && member.node() == node)
        };
        self.remove(lost, outcome);
        self.winner = Some(node);
        outcome.won = Some(node);
    }
}
