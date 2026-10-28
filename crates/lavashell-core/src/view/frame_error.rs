// SPDX-License-Identifier: GPL-3.0-only
use crate::animation::{AnimationError, BindError};
use crate::scene::tree::ApplyError;
use crate::scroll::ScrollError;
use crate::ui::{ModifierValue, Node, NodeId, Part, Used};
use crate::units::NumberError;
use std::fmt;

/// The error that stops `View::frame_at` from making a frame. A failure inside a component does not cause it.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum FrameError {
    #[error("the update of the frame does not fit the scene shown before")]
    Apply(#[source] ApplyError),
}

/// A component that failed, with the error that failed it. The root composition fails as the root node.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("component {component} failed")]
pub struct ComponentFailure {
    pub component: NodeName,
    #[source]
    pub cause: ComponentError,
}

/// The reason why a component failed.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ComponentError {
    #[error(transparent)]
    Composition(CompositionError),
    #[error("the composition never reaches a stable result: each layout makes it compose again")]
    CompositionCycle,
    #[error(transparent)]
    Layout(LayoutError),
    #[error("a transition or a state style of a node in the component cannot animate")]
    Paint(#[source] AnimationError),
    #[error("a node removed by the component cannot move to its exit")]
    Exit(#[source] AnimationError),
    #[error("a scroll state of a node in the component cannot scroll")]
    Scroll(#[source] ScrollError),
}

/// A mistake in the composed tree of nodes that makes laying out the tree impossible.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum LayoutError {
    #[error("{0} reads a NodeRef that is set on no node")]
    UnsetNodeRef(NodeValue),
    #[error("one NodeRef is set on {first} and on {second}")]
    DuplicateNodeRef { first: NodeName, second: NodeName },
    #[error(
        "{0}: a size or a condition cannot read the size of the same node, of a node around that \
         node or of a node inside it"
    )]
    RelativeSizeRead(LayoutRead),
    #[error(
        "{0}: a child with a weight is measured after the children without a weight and after the \
         conditions of the children with a weight"
    )]
    WeightedReadOrder(LayoutRead),
    #[error("layout cycle: {0}")]
    SizeCycle(LayoutReads),
    #[error(
        "layout cycle: {0}: bounds move with the offsets and the positions of their node and of the \
         nodes around it"
    )]
    BoundsCycle(LayoutReads),
    #[error("the weight of {node} is not a finite number above zero")]
    Weight {
        node: NodeName,
        #[source]
        cause: NumberError,
    },
    #[error("the {value} of {node} is not valid")]
    Modifier {
        node: NodeName,
        value: ModifierValue,
        #[source]
        cause: NumberError,
    },
    #[error("{0} has the key of a sibling: component keys must be unique within their parent")]
    DuplicateKey(NodeName),
}

impl LayoutError {
    /// The ids of the nodes that this error names.
    pub(super) fn nodes(&self) -> Vec<NodeId> {
        match self {
            Self::UnsetNodeRef(value) => vec![value.node.id],
            Self::DuplicateNodeRef { first, second } => vec![first.id, second.id],
            Self::RelativeSizeRead(read) | Self::WeightedReadOrder(read) => read.nodes().to_vec(),
            Self::SizeCycle(reads) | Self::BoundsCycle(reads) => {
                reads.0.iter().flat_map(LayoutRead::nodes).collect()
            }
            Self::Weight { node, .. } | Self::Modifier { node, .. } | Self::DuplicateKey(node) => {
                vec![node.id]
            }
        }
    }
}

/// The error that a component passes to `Ui::fail` when the component cannot compose.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CompositionError {
    #[error("a component composes a number that is not valid")]
    Number(#[from] NumberError),
    #[error("a component composes two items with one key: item keys must be unique")]
    DuplicateItemKey,
    #[error("a component binds an animation of another view")]
    Binding(#[from] BindError),
    #[error("an animation cannot take a value")]
    Animation(#[from] AnimationError),
}

/// The name of a node in an error message: the tag of the node, or the id of the node where the node has no tag.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeName {
    pub(super) id: NodeId,
    pub(super) tag: Option<String>,
}

impl NodeName {
    pub(crate) fn of(node: &Node) -> Self {
        Self {
            id: node.id,
            tag: node.tag.clone(),
        }
    }
}

impl fmt::Display for NodeName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.tag {
            Some(tag) => write!(f, "\"{tag}\""),
            None => write!(f, "{:?}", self.id),
        }
    }
}

/// A layout value of a node, named in an error message by what the node uses the value for: a length along an
/// axis, an offset, a position or a condition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeValue {
    pub(super) node: NodeName,
    pub(super) used: Used,
}

impl fmt::Display for NodeValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "the {} of {}", self.used, self.node)
    }
}

/// One read during layout: the value `reader` reads the `part` of the node `source`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayoutRead {
    pub(super) reader: NodeValue,
    pub(super) source: NodeName,
    pub(super) part: Part,
}

impl LayoutRead {
    fn nodes(&self) -> [NodeId; 2] {
        [self.reader.node.id, self.source.id]
    }
}

impl fmt::Display for LayoutRead {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} reads the {} of {}",
            self.reader, self.part, self.source
        )
    }
}

/// The layout reads that form a cycle, written as one sentence. A read continues the read before it with "which
/// reads" where the value set by the read changes the part read by the read before it. Any other read starts a new
/// clause after "and".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayoutReads(pub(super) Vec<LayoutRead>);

impl fmt::Display for LayoutReads {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut previous: Option<&LayoutRead> = None;
        for read in &self.0 {
            let chained = previous.is_some_and(|previous| {
                previous.source.id == read.reader.node.id
                    && previous.part.changes_with(read.reader.used)
            });
            if chained {
                write!(f, ", which reads the {} of {}", read.part, read.source)?;
            } else {
                if previous.is_some() {
                    f.write_str(", and ")?;
                }
                write!(f, "{read}")?;
            }
            previous = Some(read);
        }
        Ok(())
    }
}
