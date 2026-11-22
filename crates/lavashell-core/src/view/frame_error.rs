// SPDX-License-Identifier: GPL-3.0-only
use crate::animation::{AnimationError, BindError};
use crate::id_counter::IdsExhausted;
use crate::retained::UnbalancedRestore;
use crate::scene::tree::ApplyError;
use crate::scroll::ScrollError;
use crate::ui::{ModifierValue, Node, NodeId, NodeTag, Part, Used};
use crate::units::NumberError;
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum FrameError<E = ApplyError> {
    #[error("a change in the frame does not fit the scene shown before")]
    Mirror(#[source] ApplyError),
    #[error(
        "the recipient of the changes failed on a change, on the end of the frame or on a restart"
    )]
    Sink(#[source] E),
    #[error("the view painted a leaf whose commands do not make a packet")]
    Packet(#[source] UnbalancedRestore),
    #[error("every id of a scene node is used")]
    NumberSceneNode(#[source] IdsExhausted),
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("component {component} failed")]
pub struct ComponentFailure {
    pub component: NodeName,
    #[source]
    pub cause: ComponentError,
}

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
    #[error(
        "{0}: a size or a condition inside one child of a container that hides the children that \
         do not fit cannot read the size of another child or of a node inside it"
    )]
    HideableChildRead(LayoutRead),
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
    #[error("{}", invalid_modifier(.node, *.value))]
    Modifier {
        node: NodeName,
        value: ModifierValue,
        #[source]
        cause: NumberError,
    },
    #[error("{0} has the key of a sibling: component keys must be unique within their parent")]
    DuplicateKey(NodeName),
}

fn invalid_modifier(node: &NodeName, value: ModifierValue) -> String {
    match value {
        ModifierValue::MeasurementLimit => {
            format!("the measurement limit of {node} is not a finite size of zero or more")
        }
        ModifierValue::MinimumSize => format!("the minimum size of {node} is not a finite size"),
        ModifierValue::MaximumSize => {
            format!("the maximum size of {node} is neither unbounded nor a size of zero or more")
        }
        ModifierValue::Gap => format!("the gap of {node} is not a finite length of zero or more"),
        ModifierValue::BorderWidth => {
            format!("the border width of {node} is not a finite length of zero or more")
        }
        ModifierValue::Blur => format!("the blur of {node} is not a finite length of zero or more"),
        ModifierValue::OutlineWidth => {
            format!("the outline width of {node} is not a finite length of zero or more")
        }
        ModifierValue::MaterialGroupSpacing => format!(
            "the spacing of the material group of {node} is not a finite length of zero or more"
        ),
        ModifierValue::Opacity => {
            format!("the opacity of {node} is not a finite number from zero to one")
        }
    }
}

impl LayoutError {
    pub(super) fn nodes(&self) -> Vec<NodeId> {
        match self {
            Self::UnsetNodeRef(value) => vec![value.node.id],
            Self::DuplicateNodeRef { first, second } => vec![first.id, second.id],
            Self::RelativeSizeRead(read)
            | Self::WeightedReadOrder(read)
            | Self::HideableChildRead(read) => read.nodes().to_vec(),
            Self::SizeCycle(reads) | Self::BoundsCycle(reads) => {
                reads.0.iter().flat_map(LayoutRead::nodes).collect()
            }
            Self::Weight { node, .. } | Self::Modifier { node, .. } | Self::DuplicateKey(node) => {
                vec![node.id]
            }
        }
    }
}

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
    #[error("a component sets a text style, a font or a line limit on a node without text")]
    TextStyleWithoutText,
    #[error("a component sets a weight on a node placed outside the flow of its parent")]
    WeightOutOfFlow,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeName {
    pub(super) id: NodeId,
    pub(super) tag: Option<NodeTag>,
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
            Some(tag) => write!(f, "{tag:?}"),
            None => write!(f, "{:?}", self.id),
        }
    }
}

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
