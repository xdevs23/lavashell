// SPDX-License-Identifier: GPL-3.0-only
use super::{Effect, Event, NodeId, PointerSample, Recognizer, Verdict};

/// The recognizers that compete for one pressed pointer, in the order of the hit path from the
/// innermost node out, and the node that won the pointer. Once a node won, `members` contains only
/// the recognizers that won.
pub(in crate::view) struct Arena {
    /// The sample of the press, from which recognizers measure the movement of the pointer.
    press: PointerSample,
    members: Vec<Box<dyn Recognizer>>,
    /// The recognizers that wait to take the pointer from the node that won it, from the innermost node out.
    challengers: Vec<Box<dyn Recognizer>>,
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
            challengers: Vec::new(),
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
        self.recognizers().map(Recognizer::node)
    }

    pub fn recognizers(&self) -> impl Iterator<Item = &dyn Recognizer> {
        (self.members.iter().chain(&self.challengers)).map(Box::as_ref)
    }

    /// Passes `event` to the recognizers. While no recognizer has won the pointer, the first recognizer that accepts
    /// the pointer wins it. The pointer also goes to the only recognizer left after the others rejected it, and at a
    /// release to the innermost recognizer left. Once a recognizer has won the pointer, a challenger can take the
    /// pointer before the release. Then the recognizers that won the pointer run with `event`.
    pub fn step(&mut self, event: Event) -> Outcome {
        let mut outcome = Outcome::default();
        let press = self.press;
        let release = matches!(event, Event::Release { .. });
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
                remove(&mut self.members, rejected, &mut outcome);
                if self.members.len() == 1 || release && !self.members.is_empty() {
                    self.decide(0, false, &mut outcome);
                }
            }
        } else if !release {
            self.challenge(event, &mut outcome);
        }
        if self.winner.is_some() {
            for member in &mut self.members {
                outcome.effects.extend(member.run(press, event));
            }
        }
        if release {
            remove(&mut self.challengers, |_| true, &mut outcome);
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
        for members in [&mut self.members, &mut self.challengers] {
            remove(members, |member| ended(member.node()), &mut outcome);
        }
        if self.winner.is_none() && self.members.len() == 1 {
            self.decide(0, false, &mut outcome);
        }
        outcome
    }

    /// Removes every recognizer from the arena without a release.
    pub fn cancel(&mut self) -> Outcome {
        let mut outcome = Outcome::default();
        self.winner = None;
        for members in [&mut self.members, &mut self.challengers] {
            remove(members, |_| true, &mut outcome);
        }
        outcome
    }

    /// Removes the challengers for which `withdrawn` is true.
    pub fn withdraw(&mut self, withdrawn: impl Fn(&dyn Recognizer) -> bool) -> Outcome {
        let mut outcome = Outcome::default();
        remove(&mut self.challengers, withdrawn, &mut outcome);
        outcome
    }

    /// Gives the pointer to the recognizer at `index`, and to the other recognizers declared on the same node when
    /// `with_node` is true. The recognizers after it that challenge the winner wait as challengers.
    fn decide(&mut self, index: usize, with_node: bool, outcome: &mut Outcome) {
        let node = self.members[index].node();
        for (position, mut member) in std::mem::take(&mut self.members).into_iter().enumerate() {
            if position == index || with_node && member.node() == node {
                self.members.push(member);
            } else if position > index && member.challenge() {
                self.challengers.push(member);
            } else {
                outcome.effects.extend(member.cancel());
            }
        }
        self.winner = Some(node);
        outcome.won = Some(node);
    }

    /// Offers `event` to the challengers. The first challenger that accepts the pointer takes it. The recognizers
    /// that won the pointer and the challengers on the nodes inside the node of the taker then leave the arena.
    fn challenge(&mut self, event: Event, outcome: &mut Outcome) {
        let (mut taker, mut left) = (None, Vec::new());
        for mut member in std::mem::take(&mut self.challengers) {
            match member.offer(self.press, event) {
                Verdict::Accept | Verdict::AcceptWithNode if taker.is_none() => {
                    left.append(&mut self.challengers);
                    taker = Some(member);
                }
                Verdict::Reject => left.push(member),
                Verdict::Accept | Verdict::AcceptWithNode | Verdict::Wait => {
                    self.challengers.push(member);
                }
            }
        }
        if let Some(taker) = taker {
            let node = taker.node();
            left.append(&mut self.members);
            self.members.push(taker);
            self.winner = Some(node);
            outcome.won = Some(node);
        }
        remove(&mut left, |_| true, outcome);
    }
}

/// Removes the recognizers of `members` for which `removed` is true, and adds to `outcome` the changes returned by
/// `Recognizer::cancel` of each removed recognizer.
fn remove(
    members: &mut Vec<Box<dyn Recognizer>>,
    mut removed: impl FnMut(&dyn Recognizer) -> bool,
    outcome: &mut Outcome,
) {
    members.retain_mut(|member| {
        let leaves = removed(member.as_ref());
        if leaves {
            outcome.effects.extend(member.cancel());
        }
        !leaves
    });
}
