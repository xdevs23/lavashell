// SPDX-License-Identifier: GPL-3.0-only
use super::View;
use super::moving::Moving;
use super::output::GroupId;
use crate::animation::Animation;
use crate::reactive::{Phase, Scope, untracked};
use crate::scene::Affine;
use crate::scene::tree::{Clamp, Slot, TransformMotion};
use crate::scroll::ScrollState;
use crate::ui::NodeId;
use std::collections::BTreeMap;
use std::time::Duration;

#[derive(Clone)]
pub(super) struct ScrollFollower {
    pub group: GroupId,
    scroll: ScrollState,
    clamp: Clamp,
    pub component: Option<NodeId>,
    pub painted: Slot<Affine>,
}

impl ScrollFollower {
    pub fn new(group: GroupId, scroll: ScrollState, clamp: Clamp) -> Self {
        let painted = untracked(|| scroll_slot(&scroll, clamp));
        Self {
            group,
            scroll,
            clamp,
            component: None,
            painted,
        }
    }

    pub fn in_component(self, id: NodeId) -> Self {
        Self {
            component: self.component.or(Some(id)),
            ..self
        }
    }

    fn slot(&self) -> Slot<Affine> {
        scroll_slot(&self.scroll, self.clamp)
    }

    fn unsettled(&self, slot: &Slot<Affine>) -> bool {
        !slot.moves() && *slot != self.painted
    }
}

fn scroll_slot(scroll: &ScrollState, clamp: Clamp) -> Slot<Affine> {
    match scroll
        .tracked_animation()
        .as_ref()
        .and_then(Animation::playing)
    {
        Some(playing) => Slot::Motion(Box::new(TransformMotion::Clamp {
            offset: (*playing).clone(),
            clamp,
        })),
        None => Slot::Value(clamp.affine(scroll.offset())),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Presentation {
    New,
    Repeated,
}

#[derive(Default)]
pub(super) struct ScrollFollowers {
    groups: Vec<ScrollFollower>,
    scope: Scope,
    slots: BTreeMap<GroupId, Slot<Affine>>,
    presentation: Option<Duration>,
    unsettled: bool,
}

impl ScrollFollowers {
    pub fn push(&mut self, follower: ScrollFollower) {
        self.groups.push(follower);
    }

    pub fn clear(&mut self) {
        self.groups.clear();
    }

    pub fn exchange(&mut self, groups: &mut Vec<ScrollFollower>) {
        std::mem::swap(&mut self.groups, groups);
    }

    pub fn advance_presentation(&mut self, presentation: Duration) -> Presentation {
        if self.presentation.replace(presentation) == Some(presentation) {
            Presentation::Repeated
        } else {
            Presentation::New
        }
    }

    pub fn settling(&self, presentation: Presentation) -> impl Iterator<Item = NodeId> + '_ {
        let presented = presentation == Presentation::New;
        self.groups.iter().filter_map(move |follower| {
            let slot = untracked(|| follower.slot());
            let last = self.slots.get(&follower.group);
            let stable = last.is_none_or(|last| last.moves() || presented && *last == slot);
            (follower.unsettled(&slot) && stable)
                .then_some(follower.component)
                .flatten()
        })
    }

    pub fn refresh(&mut self, moving: &mut Moving) {
        let scope = self.scope.clone();
        let groups = &self.groups;
        let slots = scope.track(Phase::Paint, || {
            (groups.iter())
                .map(|follower| (follower.group, follower.slot()))
                .collect::<BTreeMap<_, _>>()
        });
        self.unsettled = (self.groups.iter()).any(|follower| {
            slots
                .get(&follower.group)
                .is_some_and(|slot| follower.unsettled(slot))
        });
        moving.rebind(&slots);
        self.slots = slots;
    }

    pub fn needs_frame(&self) -> bool {
        self.unsettled || self.scope.dirty().any()
    }

    pub fn slots(&self) -> &BTreeMap<GroupId, Slot<Affine>> {
        &self.slots
    }
}

impl Extend<ScrollFollower> for ScrollFollowers {
    fn extend<I: IntoIterator<Item = ScrollFollower>>(&mut self, groups: I) {
        self.groups.extend(groups);
    }
}

impl View {
    pub(super) fn settle_followers(&self, presentation: Presentation) {
        for id in self.followers.settling(presentation) {
            let component = (self.nodes.iter())
                .find(|node| node.id == id)
                .and_then(|node| node.component.as_ref());
            if let Some(component) = component {
                component.scope.invalidate(Phase::Paint);
            }
        }
    }
}
