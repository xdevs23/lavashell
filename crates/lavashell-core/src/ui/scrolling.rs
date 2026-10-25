// SPDX-License-Identifier: GPL-3.0-only
use super::{Modifier, Node, NodeKind, Ui};
use crate::geometry::Axis;
use crate::scene::{TEXT_STYLE, TextStyle};
use crate::scroll::{ScrollState, Scrolling};
use crate::units::Dp;

impl Ui {
    /// A row or a column, along the axis of `scrolling`, that scrolls its children.
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

/// How a node scrolls, with the text style in effect at the node. One line of that text style is the distance that
/// an arrow key scrolls.
#[derive(Clone)]
pub(crate) struct Scroller {
    scrolling: Scrolling,
    pub line: TextStyle<Dp>,
}

impl Node {
    /// The axis along which the node scrolls, or `None` for a node that does not scroll.
    pub(crate) fn scroll_axis(&self) -> Option<Axis> {
        self.scroll
            .as_ref()
            .map(|scroller| scroller.scrolling.axis())
    }

    pub(crate) fn scroll_state(&self) -> Option<&ScrollState> {
        self.scroll.as_ref()?.scrolling.state()
    }

    /// The text style in effect at a scrolling node, which sets how far an arrow key scrolls. `None` for a node
    /// that does not scroll.
    pub(crate) fn scroll_line(&self) -> Option<&TextStyle<Dp>> {
        self.scroll.as_ref().map(|scroller| &scroller.line)
    }
}
