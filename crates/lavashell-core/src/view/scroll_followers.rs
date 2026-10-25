// SPDX-License-Identifier: GPL-3.0-only
use super::moving::Moving;
use super::output::GroupId;
use super::{Affine, BTreeMap, NodeId, Phase, Scope, View};
use crate::animation::Animation;
use crate::reactive::untracked;
use crate::scene::tree::{Clamp, Slot, TransformMotion};
use crate::scroll::ScrollState;
use std::time::Duration;

/// A group whose transform follows a scroll without a new paint of the component that contains the group. Once the
/// scroll comes to rest at an offset other than the painted one, the view paints that component again.
#[derive(Clone)]
pub(super) struct ScrollFollower {
    pub group: GroupId,
    scroll: ScrollState,
    clamp: Clamp,
    /// The innermost component that painted the group, or `None` for the root, which paints on every frame.
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

    /// Sets `id` as the component that painted the group, unless a component inside `id` was set before.
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

/// Whether a frame is shown at another time than the frame before it.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Presentation {
    New,
    Repeated,
}

/// The scroll followers that the last frame painted, and their slots on that frame.
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

    /// The components of the groups whose scroll rests at a value other than the painted one. A component counts once
    /// that value was the same over a newly presented frame, or once the group came to rest since the last frame.
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

    /// Reads the transform slots of the scroll followers painted by this frame, and puts them in place of the
    /// transform slots of the matching moving groups.
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
    /// Marks for a new paint each component whose scroll follower rests at a value other than the painted one, so
    /// that the hits and the geometry of the component match the scroll.
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
