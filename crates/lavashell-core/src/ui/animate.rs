// SPDX-License-Identifier: GPL-3.0-only
use super::Ui;
use crate::animation::{Animatable, Animation, AnimationError, MotionSpec, finite};
use std::hash::Hash;
use std::rc::Rc;

impl Ui {
    pub fn animate<T: Animatable>(
        &mut self,
        key: impl Hash,
        target: T,
        spec: impl Into<MotionSpec>,
    ) -> Result<Animation<T>, AnimationError> {
        self.animate_from(key, target.clone(), target, spec.into())
    }

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
