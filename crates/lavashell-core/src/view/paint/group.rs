// SPDX-License-Identifier: GPL-3.0-only
use super::{Affine, DrawCommand, Transform, View};
use crate::animation::{Animatable, Animation, Playing};
use crate::scene::tree::{Group, Slot, SlotValue};
use crate::view::moving::GroupMotion;
use crate::view::output::{Canvas, GroupId, Painted, ScopeKind};
use crate::view::scroll_followers::ScrollFollower;
use std::rc::Rc;

#[derive(Clone, Copy)]
pub(in crate::view) struct OpenGroup(GroupMotion);

/// Whether the motions of the groups opened by a node play in the slots of those groups, where the consumer of the
/// scene animates them. Otherwise the view paints the node again whenever a value of those motions changes.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::view) enum Playback {
    InSlot,
    PerFrame,
}

impl Playback {
    pub fn in_slot_if(condition: bool) -> Self {
        if condition {
            Self::InSlot
        } else {
            Self::PerFrame
        }
    }
}

#[derive(Default)]
pub(in crate::view) struct Unwind(Vec<Closing>);

enum Closing {
    Command(DrawCommand),
    Scope,
    Group(OpenGroup),
}

impl Unwind {
    pub fn push(&mut self, command: DrawCommand) {
        self.0.push(Closing::Command(command));
    }

    pub fn scope(&mut self, canvas: &mut Canvas, scope: ScopeKind) {
        canvas.open_scope(scope);
        self.0.push(Closing::Scope);
    }

    pub fn close(&mut self, group: OpenGroup) {
        self.0.push(Closing::Group(group));
    }
}

impl Extend<DrawCommand> for Unwind {
    fn extend<I: IntoIterator<Item = DrawCommand>>(&mut self, commands: I) {
        self.0.extend(commands.into_iter().map(Closing::Command));
    }
}

impl View {
    /// The running motion of `animation`, when that motion can play in a slot.
    pub(in crate::view) fn slot_motion<T: Animatable>(
        &self,
        animation: &Animation<T>,
        playback: Playback,
    ) -> Option<Rc<Playing<T>>> {
        self.plays_in_slot(playback)
            .then(|| animation.playing())
            .flatten()
    }

    /// Whether a motion with `playback` plays in a slot. Inside a layer filtered as one image, no motion plays in a
    /// slot, because the content of that layer paints as drawing commands.
    pub(in crate::view) fn plays_in_slot(&self, playback: Playback) -> bool {
        playback == Playback::InSlot && !self.canvas.flattened()
    }

    /// The slot for `animation`, and the current value of `animation`. When the motion cannot play in the slot, the
    /// slot contains the current value, and the view paints again on every change of that value.
    pub(in crate::view) fn animated_slot<T: Animatable, U: SlotValue>(
        &self,
        animation: &Animation<T>,
        playback: Playback,
        value: impl Fn(&T) -> U,
        motion: impl FnOnce(&Playing<T>) -> U::Motion,
    ) -> (Slot<U>, T) {
        if let Some(playing) = self.slot_motion(animation, playback) {
            (Slot::Motion(Box::new(motion(&playing))), animation.peek())
        } else {
            let current = animation.get();
            (Slot::Value(value(&current)), current)
        }
    }

    /// Opens the group `id` on the canvas, and records the group as moving when `transform` animates. `outer` maps the
    /// coordinates of the parent of the group into those of the view.
    pub(in crate::view) fn open_group(
        &mut self,
        id: GroupId,
        transform: Slot<Affine>,
        opacity: Slot<f64>,
        outer: Transform,
    ) -> OpenGroup {
        let opened = (self.moving).open(id, outer, &transform, self.motion.now());
        let group = Group {
            transform,
            opacity,
            clip: None,
        };
        self.canvas.open_group(id, group);
        OpenGroup(opened)
    }

    pub(in crate::view) fn open_follower(
        &mut self,
        follower: ScrollFollower,
        outer: Transform,
    ) -> OpenGroup {
        let now = self.motion.now();
        let opened = (self.moving).follow(follower.group, outer, &follower.painted, now);
        let group = Group {
            transform: follower.painted.clone(),
            opacity: Slot::Value(1.0),
            clip: None,
        };
        self.canvas.open_group(follower.group, group);
        self.followers.push(follower);
        OpenGroup(opened)
    }

    pub(in crate::view) fn close_group(&mut self, open: OpenGroup) {
        self.moving.close(open.0);
        self.canvas.close();
    }

    /// Paints into an empty canvas from here on, and returns the canvas painted before.
    pub(in crate::view) fn open_items(&mut self) -> Canvas {
        let nested = self.canvas.nested();
        std::mem::replace(&mut self.canvas, nested)
    }

    /// Puts back the canvas returned by `open_items`, and returns the items painted since then.
    pub(in crate::view) fn close_items(&mut self, outer: Canvas) -> Vec<Painted> {
        std::mem::replace(&mut self.canvas, outer).finish()
    }

    /// An empty list of the commands and groups that close the node, reusing the memory of a list unwound before.
    pub(in crate::view) fn unwinding(&mut self) -> Unwind {
        self.spare_unwinds.pop().unwrap_or_default()
    }

    pub(in crate::view) fn unwind(&mut self, mut unwind: Unwind) {
        for closing in unwind.0.drain(..).rev() {
            match closing {
                Closing::Command(command) => self.canvas.push(command),
                Closing::Scope => self.canvas.close(),
                Closing::Group(open) => self.close_group(open),
            }
        }
        self.spare_unwinds.push(unwind);
    }
}
