// SPDX-License-Identifier: GPL-3.0-only
use super::{Modifier, Node, NodeKind, Ui};
use crate::geometry::Axis;
use crate::rebuilt::BuiltLike;
use crate::scene::{TEXT_STYLE, TextStyle};
use crate::scroll::{ScrollState, Scrolling};
use crate::units::Dp;

impl Ui {
    pub fn scroll(
        &mut self,
        scrolling: impl Into<Scrolling>,
        modifier: Modifier,
        children: impl FnOnce(&mut Self),
    ) {
        let scrolling = scrolling.into();
        let line = self.resolve_text_style(self.local(&TEXT_STYLE));
        let kind = NodeKind::Container(scrolling.axis().into());
        let mut node = self.node(self.next_id(), kind, modifier);
        node.scroll = Some(Scroller { scrolling, line });
        self.add(node, children);
    }
}

#[derive(Clone)]
pub(crate) struct Scroller {
    scrolling: Scrolling,
    pub line: TextStyle<Dp>,
}

impl BuiltLike for Scroller {
    fn is_built_like(&self, earlier: &Self) -> bool {
        self.scrolling.is_built_like(&earlier.scrolling) && self.line == earlier.line
    }
}

impl Node {
    pub(crate) fn scroll_axis(&self) -> Option<Axis> {
        self.scroll
            .as_ref()
            .map(|scroller| scroller.scrolling.axis())
    }

    pub(crate) fn scroll_state(&self) -> Option<&ScrollState> {
        self.scroll.as_ref()?.scrolling.state()
    }

    pub(crate) fn scroll_line(&self) -> Option<&TextStyle<Dp>> {
        self.scroll.as_ref().map(|scroller| &scroller.line)
    }
}
