// SPDX-License-Identifier: GPL-3.0-only
use crate::geometry::{Axis, Point, Size};
use std::cell::RefCell;
use std::hash::Hash;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PointerId {
    /// The pointer of `InputEvent::Press`, `InputEvent::Move` and `InputEvent::Release`, such as a mouse.
    Primary,
    Touch(TouchId),
}

impl PointerId {
    /// Whether the pointer remains over the view after a release, as with a mouse. A touch point
    /// exists only while it is down.
    pub(crate) fn hovers(self) -> bool {
        matches!(self, Self::Primary)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum PointerKind {
    Mouse,
    Finger,
    Pen,
}

impl PointerKind {
    /// Whether moving the pointer while it is down drags the content under it. A finger or a pen
    /// scrolls and pinches the nodes around the pressed node. A mouse drags only the pressed node.
    pub(crate) fn drags_content(self) -> bool {
        !matches!(self, Self::Mouse)
    }
}

/// A touch point. `seat` is the number given by the host to the input seat that reports the point.
/// `point` is the ID given to the point by that seat while the point is down.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TouchId {
    pub seat: u32,
    pub point: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerPhase {
    Pressed,
    /// The host sent a new position of the pointer while the press is down. The position can be
    /// the same as before.
    Moved,
    Released,
    /// The press ended without a release, such as when the window lost focus or the node was
    /// removed. The position is where the pointer was last.
    Cancelled,
}

#[derive(Clone, Copy)]
pub struct PointerInput {
    pub position: Point,
    pub size: Size,
    pub phase: PointerPhase,
    pub pointer: PointerId,
}

pub type PointerAction = Rc<RefCell<dyn FnMut(PointerInput)>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragPhase {
    /// The node captured the pointers of the drag or the pinch.
    Started,
    Moved,
    Ended,
    /// The drag or the pinch ended without a release, such as when the window lost focus, the
    /// node was removed or another node claimed one of the touch points of the pinch.
    Cancelled,
}

#[derive(Clone, Copy, Debug)]
pub struct DragInput {
    pub position: Point,
    pub size: Size,
    pub phase: DragPhase,
    pub pointer: PointerId,
}

pub type DragHandler = Rc<RefCell<dyn FnMut(DragInput)>>;

/// The drag declared on a node with `on_drag`.
#[derive(Clone)]
pub struct Draggable {
    pub axis: Axis,
    pub handler: DragHandler,
}

/// Two touch points that pinch a node.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransformInput {
    /// The point between the touch points, relative to the node.
    pub focal: Point,
    /// The current distance between the touch points divided by the distance between them when
    /// the pinch started.
    pub scale: f64,
    pub phase: DragPhase,
}

pub type TransformHandler = Rc<RefCell<dyn FnMut(TransformInput)>>;
