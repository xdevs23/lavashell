// SPDX-License-Identifier: GPL-3.0-only

use super::View;
use crate::geometry::Rect;
use crate::style::InteractionState;
use crate::test_error::TestError;
use crate::ui::{NodeTag, ViewTag};

pub(crate) trait Tagged {
    fn tagged_bounds<T: ViewTag + Clone>(&self, tag: &T) -> Result<Rect, TestError>;

    fn tagged_state<T: ViewTag + Clone>(&self, tag: &T) -> Result<InteractionState, TestError>;
}

impl Tagged for View {
    fn tagged_bounds<T: ViewTag + Clone>(&self, tag: &T) -> Result<Rect, TestError> {
        self.bounds_of(tag)
            .ok_or_else(|| TestError::NoTag(NodeTag::new(tag.clone())))
    }

    fn tagged_state<T: ViewTag + Clone>(&self, tag: &T) -> Result<InteractionState, TestError> {
        self.interaction_state(tag)
            .ok_or_else(|| TestError::NoTag(NodeTag::new(tag.clone())))
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
            view.tagged_bounds(&"absent"),
            Err(TestError::NoTag(tag)) if tag.is(&"absent")
        ));
        assert!(matches!(
            view.tagged_state(&"absent"),
            Err(TestError::NoTag(_))
        ));
    }
}
