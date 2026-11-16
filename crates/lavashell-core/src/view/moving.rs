// SPDX-License-Identifier: GPL-3.0-only
use super::output::GroupId;
use crate::geometry::{Point, Rect};
use crate::scene::tree::Slot;
use crate::scene::{Affine, Transform};
use std::collections::BTreeMap;
use std::time::Duration;

#[derive(Clone)]
pub(super) struct MovingGroup {
    id: GroupId,
    parent: Option<GroupId>,
    outer: Transform,
    transform: Slot<Affine>,
    painted: Transform,
}

impl MovingGroup {
    fn at(&self, affine: Affine) -> Transform {
        Transform::try_of(affine).unwrap_or(self.painted)
    }

    fn moved(&self, at: Transform) -> Transform {
        if at == self.painted {
            return Transform::IDENTITY;
        }
        (self.outer.then(at))
            .then(self.painted.inverse())
            .then(self.outer.inverse())
    }

    fn at_rest(&self) -> Transform {
        self.moved(self.at(self.transform.end()))
    }

    fn shown_at(&self, now: Duration) -> Transform {
        self.moved(self.at(self.transform.at(now)))
    }
}

#[derive(Default)]
pub(super) struct Moving {
    groups: Vec<MovingGroup>,
    pub current: Option<GroupId>,
    shown: BTreeMap<GroupId, Transform>,
    around: Transform,
}

#[derive(Clone, Copy)]
pub(super) enum GroupMotion {
    Still,
    Moving { around: Option<GroupId> },
}

impl Moving {
    pub fn open(
        &mut self,
        id: GroupId,
        outer: Transform,
        transform: &Slot<Affine>,
        now: Duration,
    ) -> GroupMotion {
        if !transform.moves() {
            return GroupMotion::Still;
        }
        self.follow(id, outer, transform, now)
    }

    pub fn follow(
        &mut self,
        id: GroupId,
        outer: Transform,
        transform: &Slot<Affine>,
        now: Duration,
    ) -> GroupMotion {
        let Some(painted) = Transform::try_of(transform.at(now)) else {
            return GroupMotion::Still;
        };
        self.groups.push(MovingGroup {
            id,
            parent: self.current,
            outer,
            transform: transform.clone(),
            painted,
        });
        GroupMotion::Moving {
            around: self.current.replace(id),
        }
    }

    pub fn close(&mut self, opened: GroupMotion) {
        if let GroupMotion::Moving { around } = opened {
            self.current = around;
        }
    }

    pub fn into_groups(self) -> Vec<MovingGroup> {
        self.groups
    }

    pub fn inside(&self) -> Self {
        Self {
            around: self.resting(),
            ..Self::default()
        }
    }

    pub fn resting(&self) -> Transform {
        self.around.then(self.rest_of(self.current))
    }

    pub fn at_rest(&self, transform: Transform) -> Transform {
        self.resting().then(transform)
    }

    fn rest_of(&self, group: Option<GroupId>) -> Transform {
        let mut rest = Transform::IDENTITY;
        let mut wanted = group;
        for group in self.groups.iter().rev() {
            if wanted == Some(group.id) {
                rest = group.at_rest().then(rest);
                wanted = group.parent;
            }
        }
        rest
    }

    pub fn place(&mut self, groups: &[MovingGroup], moved: Transform) {
        let around = self.current;
        self.groups.extend(groups.iter().map(|group| MovingGroup {
            parent: group.parent.or(around),
            outer: moved.then(group.outer),
            ..group.clone()
        }));
    }

    pub fn rebind(&mut self, slots: &BTreeMap<GroupId, Slot<Affine>>) {
        for group in &mut self.groups {
            if let Some(slot) = slots.get(&group.id) {
                group.transform = slot.clone();
            }
        }
    }

    pub fn present(&mut self, now: Duration) {
        self.shown.clear();
        for group in &self.groups {
            let parent = (group.parent).map_or(Transform::IDENTITY, |parent| self.shown[&parent]);
            let _none_after_the_clear = self
                .shown
                .insert(group.id, parent.then(group.shown_at(now)));
        }
    }

    pub fn painted(&self, group: Option<GroupId>, at: Point) -> Point {
        self.map(group)
            .map_or(at, |shown| shown.inverse().point(at))
    }

    pub fn shown(&self, group: Option<GroupId>, rect: Rect) -> Rect {
        self.map(group).map_or(rect, |shown| shown.rect(rect))
    }

    pub fn rested(&self, group: Option<GroupId>, rect: Rect) -> Rect {
        self.rest_of(group).rect(rect)
    }

    fn map(&self, group: Option<GroupId>) -> Option<Transform> {
        group.map(|group| self.shown[&group])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::animation::{Animatable, MotionSpec, Playing, Program, Timing, Tween, Vector};
    use crate::geometry::Viewport;
    use crate::scene::tree::TransformMotion;
    use crate::test_error::TestResult;
    use crate::ui::NodeId;
    use crate::units::Dp;

    #[test]
    fn input_maps_to_where_a_group_was_painted_while_its_slot_rotates() -> TestResult {
        let turn = Affine::rotate(std::f64::consts::FRAC_PI_2);
        let program = Program::Target {
            from: Affine::IDENTITY.vector(),
            to: turn.vector(),
            velocity: Vector([0.0; 6]),
            spec: MotionSpec::Tween(Tween::new(Duration::from_millis(100))),
            step: Affine::visible_step(Viewport::default().pixel()),
        };
        let timing = Timing::from_start(Duration::ZERO);
        let playing = Playing::new(turn, program, timing)?;
        let slot = Slot::Motion(Box::new(TransformMotion::Affine(playing)));
        let group = GroupId::Content(NodeId(1));
        let mut moving = Moving::default();
        let opened = moving.follow(group, Transform::IDENTITY, &slot, Duration::ZERO);
        moving.close(opened);
        moving.present(Duration::from_millis(50));
        let at = Point::new(Dp::new(10.0), Dp::new(20.0));
        assert_eq!(moving.painted(Some(group), at), at);
        Ok(())
    }
}
