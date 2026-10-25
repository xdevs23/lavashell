// SPDX-License-Identifier: GPL-3.0-only
//! Lookups for tests of the bounds and of the interaction state of the node that has a given tag in
//! a view. A tag that no node has fails the test with `TestError::NoTag`.

use super::{Rect, View};
use crate::style::InteractionState;
use crate::test_error::TestError;

pub(crate) trait Tagged {
    fn tagged_bounds(&self, tag: &str) -> Result<Rect, TestError>;

    fn tagged_state(&self, tag: &str) -> Result<InteractionState, TestError>;
}

impl Tagged for View {
    fn tagged_bounds(&self, tag: &str) -> Result<Rect, TestError> {
        self.bounds_of(tag)
            .ok_or_else(|| TestError::NoTag(tag.to_owned()))
    }

    fn tagged_state(&self, tag: &str) -> Result<InteractionState, TestError> {
        self.interaction_state(tag)
            .ok_or_else(|| TestError::NoTag(tag.to_owned()))
    }
}

mod tests {
    use super::Tagged;
    use crate::test_error::TestError;
    use crate::view::View;

    #[test]
    fn a_tag_that_no_node_has_fails_with_that_tag() {
        let view = View::new(crate::platform::fixture::facts(), |_| {});
        assert!(matches!(
            view.tagged_bounds("absent"),
            Err(TestError::NoTag(tag)) if tag == "absent"
        ));
        assert!(matches!(
            view.tagged_state("absent"),
            Err(TestError::NoTag(_))
        ));
    }
}
