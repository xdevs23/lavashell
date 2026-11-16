// SPDX-License-Identifier: GPL-3.0-only
use super::frame_error::ComponentFailure;

#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failures(pub(super) Vec<ComponentFailure>);

impl IntoIterator for Failures {
    type Item = ComponentFailure;
    type IntoIter = std::vec::IntoIter<ComponentFailure>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl Failures {
    pub fn into_result(self) -> Result<(), Vec<ComponentFailure>> {
        if self.0.is_empty() {
            Ok(())
        } else {
            Err(self.0)
        }
    }
}
