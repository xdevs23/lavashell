// SPDX-License-Identifier: GPL-3.0-only
use super::Playback;
use super::group::{OpenGroup, Unwind};
use crate::geometry::{Point, Rect};
use crate::reactive::untracked;
use crate::scene::tree::{Clamp, Slot};
use crate::scene::{Affine, Transform};
use crate::scroll::ScrollState;
use crate::shape::{Corners, Radius};
use crate::units::Dp;
use crate::view::output::{GroupId, ScopeKind};
use crate::view::scroll::content_clamp;
use crate::view::scroll_followers::ScrollFollower;
use crate::view::{HitClip, View};

pub(super) struct Scrolled {
    slot: ScrolledSlot,
    pub shift: Point,
}

enum ScrolledSlot {
    Painted(Slot<Affine>),
    Follows(ScrollFollower),
}

impl Scrolled {
    fn value(clamp: Clamp, offset: Dp) -> Self {
        Self {
            slot: ScrolledSlot::Painted(Slot::Value(clamp.affine(offset))),
            shift: clamp.translation(offset),
        }
    }

    pub fn resting(&self) -> Point {
        let slot = match &self.slot {
            ScrolledSlot::Painted(slot) => slot,
            ScrolledSlot::Follows(follower) => &follower.painted,
        };
        slot.end().translation
    }
}

impl View {
    pub(super) fn scrolled(
        &self,
        group: GroupId,
        state: &ScrollState,
        clamp: Clamp,
        playback: Playback,
    ) -> Scrolled {
        if state.animation().is_some() && self.painting.plays_in_slot(playback) {
            let offset = untracked(|| state.offset());
            return Scrolled {
                slot: ScrolledSlot::Follows(ScrollFollower::new(group, state.clone(), clamp)),
                shift: clamp.translation(offset),
            };
        }
        Scrolled::value(clamp, state.offset())
    }

    pub(super) fn open_scrolled(
        &mut self,
        group: GroupId,
        scrolled: Scrolled,
        outer: Transform,
    ) -> OpenGroup {
        match scrolled.slot {
            ScrolledSlot::Painted(slot) => self.open_group(group, slot, Slot::Value(1.0), outer),
            ScrolledSlot::Follows(follower) => self.open_follower(follower, outer),
        }
    }

    pub(super) fn scroll_content(&mut self, index: usize, playback: Playback) -> Option<Scrolled> {
        let node = &self.nodes[index];
        let clamp = content_clamp(node, &self.layout[index])?;
        if let Some(state) = node.scroll_state() {
            let group = GroupId::Content(node.id);
            return Some(self.scrolled(group, &state.clone(), clamp, playback));
        }
        let offset = self.scrolls.entry(node.id).or_default();
        *offset = offset.clamp(clamp.min, clamp.max);
        Some(Scrolled::value(clamp, *offset))
    }

    pub(super) fn follow_scroll(
        &self,
        group: GroupId,
        scroll: &ScrollState,
        travel: Dp,
        playback: Playback,
    ) -> Scrolled {
        let max = scroll.maximum();
        let clamp = Clamp {
            min: Dp::ZERO,
            max,
            axis: scroll.axis(),
            scale: if max > Dp::ZERO { travel / max } else { 0.0 },
        };
        self.scrolled(group, scroll, clamp, playback)
    }

    pub(super) fn clip_scope(
        &mut self,
        rect: Rect,
        corners: Corners<Radius>,
        transform: Transform,
        clips: &mut Vec<HitClip>,
        unwind: &mut Unwind,
    ) {
        let clip = HitClip::new(rect, corners, transform);
        clips.push(clip.in_group(self.routing.moving.current));
        unwind.scope(&mut self.painting.canvas, ScopeKind::clip(rect, corners));
    }

    pub(super) fn open_content(
        &mut self,
        index: usize,
        scrolled: Scrolled,
        transform: Transform,
    ) -> (OpenGroup, Transform) {
        let shift = scrolled.shift;
        let group = GroupId::Content(self.nodes[index].id);
        let open = self.open_scrolled(group, scrolled, transform);
        (open, transform.then(Transform::translation(shift)))
    }
}
