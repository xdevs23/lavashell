// SPDX-License-Identifier: GPL-3.0-only
use super::{
    Animatable, Animation, AnimationError, MotionSpec, Program, Scheduled, VectorSpace, finite,
};

impl<T: Animatable> Animation<T> {
    pub(crate) fn shift_or_animate_to(
        &self,
        target: T,
        spec: impl Into<MotionSpec>,
    ) -> Result<(), AnimationError> {
        finite(&target)?;
        self.0.synchronize();
        let mut t = self.0.track.borrow_mut();
        let running = (t.program.as_ref().map(Scheduled::program)).filter(|_| !t.finished);
        let Some(Program::Target {
            from,
            to,
            velocity,
            spec: running_spec,
            step,
        }) = running
        else {
            drop(t);
            return self.animate_to(target, spec);
        };
        let shift = target.vector().sub(&t.target.vector());
        let shifted = Program::Target {
            from: from.add(&shift),
            to: to.add(&shift),
            velocity: velocity.clone(),
            spec: *running_spec,
            step: *step,
        };
        t.run(shifted);
        t.end_value = t.end_value.at(t.end_value.vector().add(&shift));
        t.target = target;
        t.sample();
        self.0.publish(t);
        self.0.revise();
        Ok(())
    }
}
