// SPDX-License-Identifier: GPL-3.0-only
use crate::animation::{Animatable, Animation, Bindable};
use crate::geometry::{Axis, Rect, Size};
use crate::units::Dp;
use laid_out::{Measured, Placed};
use std::fmt;
use std::rc::Rc;

/// A value used in layout that may depend on other nodes: a constant, the size or the bounds of a
/// node, an animation, or a value computed from those. The layout computes the value once the nodes
/// it depends on are laid out. `R` sets what the value may depend on: [`Sizes`] for a width or a
/// height, or [`Bounds`] for an offset or a position.
pub struct Deferred<T, R: Reach = Sizes>(Source<T, R>);

enum Source<T, R: Reach> {
    Constant(T),
    Computed(Rc<Computed<T, R>>),
}

struct Computed<T, R: Reach> {
    reads: Vec<Read>,
    resolve: Resolve<T, R>,
}

type Resolve<T, R> = Box<dyn for<'a> Fn(&mut <R as Reach>::LaidOut<'a>) -> T>;

/// What a [`Deferred`] value may depend on.
pub trait Reach: 'static {
    #[doc(hidden)]
    type LaidOut<'a>: Measured + ?Sized;
}

/// A [`Deferred`] value that depends on the sizes of nodes and on animations.
pub enum Sizes {}

/// A [`Deferred`] value that depends on the bounds and the sizes of nodes and on animations.
pub enum Bounds {}

impl Reach for Sizes {
    type LaidOut<'a> = dyn Measured + 'a;
}

impl Reach for Bounds {
    type LaidOut<'a> = dyn Placed + 'a;
}

impl<T: Clone, R: Reach> Clone for Deferred<T, R> {
    fn clone(&self) -> Self {
        Self(match &self.0 {
            Source::Constant(value) => Source::Constant(value.clone()),
            Source::Computed(computed) => Source::Computed(computed.clone()),
        })
    }
}

impl<T, R: Reach> From<T> for Deferred<T, R> {
    fn from(value: T) -> Self {
        Self(Source::Constant(value))
    }
}

impl<T: Animatable, R: Reach> From<&Animation<T>> for Deferred<T, R> {
    fn from(animation: &Animation<T>) -> Self {
        let bound: Rc<dyn Bindable> = Rc::new(animation.clone());
        let animation = animation.clone();
        Self::computed(vec![Read::Animation(bound)], move |_| animation.get())
    }
}

impl<T: 'static> From<Deferred<T>> for Deferred<T, Bounds> {
    fn from(value: Deferred<T>) -> Self {
        match value.0 {
            Source::Constant(value) => Self(Source::Constant(value)),
            Source::Computed(computed) => {
                let reads = computed.reads.clone();
                Self::computed(reads, move |layout| (computed.resolve)(layout))
            }
        }
    }
}

impl<T: Clone + 'static, R: Reach> Deferred<T, R> {
    pub fn map<U>(&self, f: impl Fn(T) -> U + 'static) -> Deferred<U, R> {
        if let Source::Constant(value) = &self.0 {
            return Deferred(Source::Constant(f(value.clone())));
        }
        let source = self.clone();
        Deferred::computed(self.reads().to_vec(), move |layout| {
            f(source.resolve(layout))
        })
    }

    pub fn zip<U: Clone + 'static>(&self, other: &Deferred<U, R>) -> Deferred<(T, U), R> {
        if let (Source::Constant(first), Source::Constant(second)) = (&self.0, &other.0) {
            return Deferred(Source::Constant((first.clone(), second.clone())));
        }
        let (first, second) = (self.clone(), other.clone());
        Deferred::computed([self.reads(), other.reads()].concat(), move |layout| {
            (first.resolve(layout), second.resolve(layout))
        })
    }

    pub(crate) fn resolve(&self, layout: &mut R::LaidOut<'_>) -> T {
        match &self.0 {
            Source::Constant(value) => value.clone(),
            Source::Computed(computed) => (computed.resolve)(layout),
        }
    }
}

impl<T, R: Reach> Deferred<T, R> {
    fn computed(
        reads: Vec<Read>,
        resolve: impl for<'a> Fn(&mut R::LaidOut<'a>) -> T + 'static,
    ) -> Self {
        Self(Source::Computed(Rc::new(Computed {
            reads,
            resolve: Box::new(resolve),
        })))
    }

    pub(crate) fn reads(&self) -> &[Read] {
        match &self.0 {
            Source::Constant(_) => &[],
            Source::Computed(computed) => &computed.reads,
        }
    }
}

/// The results of layout that a deferred value reads for the nodes it depends on.
pub(crate) mod laid_out {
    use super::NodeRef;
    use crate::geometry::{Rect, Size};
    use crate::units::Dp;

    pub trait Measured {
        fn size(&mut self, node: &NodeRef) -> Size;

        fn natural_width(&mut self, node: &NodeRef) -> Dp;
    }

    /// The bounds of the nodes placed so far, with the sizes of every node.
    pub trait Placed: Measured {
        fn bounds(&mut self, node: &NodeRef) -> Rect;
    }
}

#[derive(Clone)]
pub(crate) enum Read {
    Node(NodeRef, Part),
    Animation(Rc<dyn Bindable>),
}

/// The part of a node that a deferred value depends on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Part {
    Length(Axis),
    Size,
    NaturalWidth,
    Bounds,
}

/// The layout property of a node that a deferred value sets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Used {
    Length(Axis),
    Offset,
    Position,
    Condition,
}

impl Part {
    /// Whether this part of a node changes when the node sets its `used` property from a deferred value.
    pub fn changes_with(self, used: Used) -> bool {
        match (self, used) {
            (Self::Length(read), Used::Length(set)) => read == set,
            (Self::NaturalWidth, Used::Length(set)) => set == Axis::Vertical,
            (Self::Size, Used::Length(_))
            | (Self::Length(_) | Self::Size, Used::Condition)
            | (Self::Bounds, _) => true,
            (Self::Length(_) | Self::Size | Self::NaturalWidth, Used::Offset | Used::Position)
            | (Self::NaturalWidth, Used::Condition) => false,
        }
    }
}

fn length(axis: Axis) -> &'static str {
    match axis {
        Axis::Horizontal => "width",
        Axis::Vertical => "height",
    }
}

impl fmt::Display for Part {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Length(axis) => length(*axis),
            Self::Size => "size",
            Self::NaturalWidth => "natural width",
            Self::Bounds => "bounds",
        })
    }
}

impl fmt::Display for Used {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Length(axis) => length(*axis),
            Self::Offset => "offset",
            Self::Position => "position",
            Self::Condition => "condition",
        })
    }
}

/// A reference to a node, for deferred values that depend on the size or the bounds of that node.
/// The reference can be made before or after the node is composed.
#[derive(Clone, Default)]
pub struct NodeRef(Rc<()>);

/// The identity of a [`NodeRef`], shared by its clones.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct NodeRefKey(usize);

impl PartialEq for NodeRef {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl NodeRef {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn width(&self) -> Deferred<Dp> {
        self.read(Part::Length(Axis::Horizontal), |layout, node| {
            layout.size(node).width
        })
    }

    #[must_use]
    pub fn height(&self) -> Deferred<Dp> {
        self.read(Part::Length(Axis::Vertical), |layout, node| {
            layout.size(node).height
        })
    }

    #[must_use]
    pub fn size(&self) -> Deferred<Size> {
        self.read(Part::Size, |layout, node| layout.size(node))
    }

    /// The width of the node at an unbounded proposal, before the width set on it.
    #[must_use]
    pub fn natural_width(&self) -> Deferred<Dp> {
        self.read(Part::NaturalWidth, |layout, node| {
            layout.natural_width(node)
        })
    }

    /// The bounds of the node in the view after layout, with scrolling and offsets applied.
    #[must_use]
    pub fn bounds(&self) -> Deferred<Rect, Bounds> {
        let node = self.clone();
        let reads = vec![Read::Node(node.clone(), Part::Bounds)];
        Deferred::<_, Bounds>::computed(reads, move |layout| layout.bounds(&node))
    }

    pub(crate) fn key(&self) -> NodeRefKey {
        NodeRefKey(Rc::as_ptr(&self.0).addr())
    }

    fn read<T: 'static>(
        &self,
        part: Part,
        read: fn(&mut (dyn Measured + '_), &NodeRef) -> T,
    ) -> Deferred<T> {
        let node = self.clone();
        Deferred::computed(vec![Read::Node(node.clone(), part)], move |layout| {
            read(layout, &node)
        })
    }
}
