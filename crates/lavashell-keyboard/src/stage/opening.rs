// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::prelude::{Animatable, Dp, Trajectory, Vector, VectorSpace};

/// How far the panel is open, from 0 while it is hidden to 1 while it is shown. The animation moves the
/// uncovered height of the panel in dp, so the motion ends once the change of that height is too small to see.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Opening {
    share: f64,
    panel: Dp,
}

impl Opening {
    pub fn new(share: f64, panel: Dp) -> Self {
        Self { share, panel }
    }

    pub fn share(self) -> f64 {
        self.share
    }
}

impl Animatable for Opening {
    type Vector = Vector<1>;

    fn vector(&self) -> Vector<1> {
        (self.panel * self.share).vector()
    }

    fn visible_step(pixel: Dp) -> f64 {
        Dp::visible_step(pixel)
    }

    /// A panel without height uncovers no length, so `uncovered` cannot give a share. The share of this
    /// value is returned unchanged.
    fn at(&self, uncovered: Vector<1>) -> Self {
        let share = if self.panel > Dp::ZERO {
            Dp::ZERO.at(uncovered) / self.panel
        } else {
            self.share
        };
        Self::new(share, self.panel)
    }

    /// Moves a running animation from the panel height of `previous` to the panel height of this value.
    /// The share and the speed of the share do not change.
    fn retarget(&self, previous: &Self, current: Trajectory<Vector<1>>) -> Trajectory<Vector<1>> {
        let velocity = if previous.panel > Dp::ZERO {
            current.velocity.scale(self.panel / previous.panel)
        } else {
            Vector::zero()
        };
        let share = previous.at(current.position).share;
        Trajectory {
            position: Self::new(share, self.panel).vector(),
            velocity,
            end: self.vector(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lavashell_core::prelude::DpExt;

    #[test]
    fn an_opening_keeps_its_share_and_the_speed_of_its_share_on_a_panel_of_another_height() {
        let (low, high) = (Opening::new(1.0, 200.dp()), Opening::new(0.0, 400.dp()));
        let moving = Trajectory {
            position: Vector([50.0]),
            velocity: Vector([100.0]),
            end: low.vector(),
        };
        let carried = high.retarget(&low, moving);
        assert_eq!(carried.position, Vector([100.0]));
        assert_eq!(carried.velocity, Vector([200.0]));
        assert_eq!(carried.end, Vector([0.0]));
    }

    #[test]
    fn a_panel_without_height_is_at_the_share_of_its_end() {
        let shown = Opening::new(1.0, Dp::ZERO);
        assert_eq!(shown.at(Vector([0.0])), shown);
        let halfway = Trajectory {
            position: Vector([150.0]),
            velocity: Vector([300.0]),
            end: Vector([0.0]),
        };
        let carried = shown.retarget(&Opening::new(0.0, 300.dp()), halfway);
        assert_eq!(carried.velocity, Vector([0.0]));
        assert_eq!(carried.position, carried.end);
    }
}
