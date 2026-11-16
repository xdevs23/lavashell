// SPDX-License-Identifier: GPL-3.0-only
use crate::animation::{Animatable, Animation, Playing};
use crate::scene::tree::{Group, Slot, SlotValue};
use crate::scene::{Affine, DrawCommand, Transform};
use crate::view::View;
use crate::view::moving::GroupMotion;
use crate::view::output::{Canvas, GroupId, Painted, ScopeKind};
use crate::view::paint::PaintOutput;
use crate::view::scroll_followers::ScrollFollower;
use std::rc::Rc;

#[derive(Clone, Copy)]
pub(in crate::view) struct OpenGroup(GroupMotion);

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
    pub(in crate::view) fn slot_motion<T: Animatable>(
        &self,
        animation: &Animation<T>,
        playback: Playback,
    ) -> Option<Rc<Playing<T>>> {
        self.painting
            .plays_in_slot(playback)
            .then(|| animation.playing())
            .flatten()
    }

    pub(in crate::view) fn animated_slot<T: Animatable, U: SlotValue>(
        &self,
        animation: &Animation<T>,
        playback: Playback,
        value: impl Fn(&T) -> U,
        motion: impl FnOnce(&Playing<T>) -> U::Motion,
    ) -> Slot<U> {
        match self.slot_motion(animation, playback) {
            Some(playing) => Slot::Motion(Box::new(motion(&playing))),
            None => Slot::Value(value(&animation.get())),
        }
    }

    pub(in crate::view) fn open_group(
        &mut self,
        id: GroupId,
        transform: Slot<Affine>,
        opacity: Slot<f64>,
        outer: Transform,
    ) -> OpenGroup {
        let opened = (self.routing.moving).open(id, outer, &transform, self.motion.now());
        let group = Group {
            transform,
            opacity,
            clip: None,
        };
        self.painting.canvas.open_group(id, group);
        OpenGroup(opened)
    }

    pub(in crate::view) fn open_follower(
        &mut self,
        follower: ScrollFollower,
        outer: Transform,
    ) -> OpenGroup {
        let now = self.motion.now();
        let opened = (self.routing.moving).follow(follower.group, outer, &follower.painted, now);
        let group = Group {
            transform: follower.painted.clone(),
            opacity: Slot::Value(1.0),
            clip: None,
        };
        self.painting.canvas.open_group(follower.group, group);
        self.followers.push(follower);
        OpenGroup(opened)
    }

    pub(in crate::view) fn close_group(&mut self, open: OpenGroup) {
        self.routing.moving.close(open.0);
        self.painting.canvas.close();
    }

    pub(in crate::view) fn unwind(&mut self, mut unwind: Unwind) {
        for closing in unwind.0.drain(..).rev() {
            match closing {
                Closing::Command(command) => self.painting.canvas.push(command),
                Closing::Scope => self.painting.canvas.close(),
                Closing::Group(open) => self.close_group(open),
            }
        }
        self.painting.spare_unwinds.push(unwind);
    }
}

impl PaintOutput {
    pub(in crate::view) fn plays_in_slot(&self, playback: Playback) -> bool {
        playback == Playback::InSlot && !self.canvas.flattened()
    }
    pub(in crate::view) fn open_items(&mut self) -> Canvas {
        let nested = self.canvas.nested();
        std::mem::replace(&mut self.canvas, nested)
    }
    pub(in crate::view) fn close_items(&mut self, outer: Canvas) -> Vec<Painted> {
        std::mem::replace(&mut self.canvas, outer).finish()
    }
    pub(in crate::view) fn unwinding(&mut self) -> Unwind {
        self.spare_unwinds.pop().unwrap_or_default()
    }
}
