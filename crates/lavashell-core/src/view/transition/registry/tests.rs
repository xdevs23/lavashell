// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::test_error::{Required, TestError};

impl Transitions {
    pub fn count(&self) -> usize {
        self.states.len()
    }

    pub fn only_animation(&self) -> Result<&Animation<Visual>, TestError> {
        assert_eq!(self.count(), 1, "the view has one transitioned node");
        let state = self
            .states
            .values()
            .next()
            .required("a transitioned node")?;
        Ok(&state.animation)
    }
}
