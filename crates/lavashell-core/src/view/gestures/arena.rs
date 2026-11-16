// SPDX-License-Identifier: GPL-3.0-only
use super::{Effect, Event, Recognizer, Verdict};
use crate::ui::NodeId;
use crate::view::pointers::PointerSample;

pub(in crate::view) struct Arena {
    press: PointerSample,
    members: Vec<Box<dyn Recognizer>>,
    challengers: Vec<Box<dyn Recognizer>>,
    winner: Option<NodeId>,
}

#[derive(Default)]
pub(in crate::view) struct Outcome {
    pub won: Option<NodeId>,
    pub effects: Vec<Effect>,
}

#[derive(Clone, Copy)]
enum Winner {
    Recognizer,
    Node,
}

impl Arena {
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

    pub fn capture(&self) -> Option<NodeId> {
        Some(self.members.first()?.node())
    }

    pub fn nodes(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.recognizers().map(Recognizer::node)
    }

    pub fn recognizers(&self) -> impl Iterator<Item = &dyn Recognizer> {
        (self.members.iter().chain(&self.challengers)).map(Box::as_ref)
    }

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
                    Verdict::Accept => Some((index, Winner::Recognizer)),
                    Verdict::AcceptWithNode => Some((index, Winner::Node)),
                    Verdict::Wait | Verdict::Reject => None,
                });
            if let Some((index, winner)) = accepted {
                self.decide(index, winner, &mut outcome);
            } else {
                let mut verdicts = verdicts.into_iter();
                let rejected =
                    |_: &dyn Recognizer| matches!(verdicts.next(), Some(Verdict::Reject));
                remove(&mut self.members, rejected, &mut outcome);
                if self.members.len() == 1 || release && !self.members.is_empty() {
                    self.decide(0, Winner::Recognizer, &mut outcome);
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

    pub fn end(&mut self, ended: impl Fn(NodeId) -> bool) -> Outcome {
        if self.winner.is_some_and(&ended) {
            return self.cancel();
        }
        let mut outcome = Outcome::default();
        for members in [&mut self.members, &mut self.challengers] {
            remove(members, |member| ended(member.node()), &mut outcome);
        }
        if self.winner.is_none() && self.members.len() == 1 {
            self.decide(0, Winner::Recognizer, &mut outcome);
        }
        outcome
    }

    pub fn cancel(&mut self) -> Outcome {
        let mut outcome = Outcome::default();
        self.winner = None;
        for members in [&mut self.members, &mut self.challengers] {
            remove(members, |_| true, &mut outcome);
        }
        outcome
    }

    pub fn withdraw(&mut self, withdrawn: impl Fn(&dyn Recognizer) -> bool) -> Outcome {
        let mut outcome = Outcome::default();
        remove(&mut self.challengers, withdrawn, &mut outcome);
        outcome
    }

    fn decide(&mut self, index: usize, winner: Winner, outcome: &mut Outcome) {
        let node = self.members[index].node();
        for (position, mut member) in std::mem::take(&mut self.members).into_iter().enumerate() {
            let wins = match winner {
                Winner::Recognizer => position == index,
                Winner::Node => member.node() == node,
            };
            if wins {
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
