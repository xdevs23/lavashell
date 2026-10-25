// SPDX-License-Identifier: GPL-3.0-only
use super::Ui;
use crate::animation::{Animatable, Animation, AnimationError, MotionSpec, finite};
use std::hash::Hash;
use std::rc::Rc;

impl Ui {
    /// The animation stored under `key`, moved toward `target` with `spec` on every composition. To change
    /// only the painting, read its value while a canvas paints. To change the layout, pass it to a width, a
    /// height or an offset.
    ///
    /// # Errors
    ///
    /// Fails when `target` is not finite.
    pub fn animate<T: Animatable>(
        &mut self,
        key: impl Hash,
        target: T,
        spec: impl Into<MotionSpec>,
    ) -> Result<Animation<T>, AnimationError> {
        self.animate_from(key, target.clone(), target, spec.into())
    }

    /// An animation like [`Ui::animate`] that starts at `gone` on its first composition and moves
    /// to `shown`. When the node in which it is composed is removed and plays its exit transition,
    /// the animation moves back to `gone`.
    ///
    /// # Errors
    ///
    /// Fails when `shown` or `gone` is not finite.
    pub fn animate_presence<T: Animatable>(
        &mut self,
        key: impl Hash,
        shown: T,
        gone: T,
        spec: impl Into<MotionSpec>,
    ) -> Result<Animation<T>, AnimationError> {
        let spec = spec.into();
        finite(&gone)?;
        let animation = self.animate_from(key, gone.clone(), shown, spec)?;
        let leaving = animation.clone();
        let exit = move || leaving.animate_to(gone.clone(), spec);
        let parent = self.parent;
        self.nodes[parent].on_exit.push(Rc::new(exit));
        Ok(animation)
    }

    fn animate_from<T: Animatable>(
        &mut self,
        key: impl Hash,
        start: T,
        target: T,
        spec: MotionSpec,
    ) -> Result<Animation<T>, AnimationError> {
        let remembered = self.state(key, || None::<Animation<T>>);
        let animation = if let Some(animation) = remembered.peek() {
            animation
        } else {
            let animation = self.motion.animation(start)?;
            remembered.set(Some(animation.clone()));
            animation
        };
        animation.animate_to(target, spec)?;
        Ok(animation)
    }
}
