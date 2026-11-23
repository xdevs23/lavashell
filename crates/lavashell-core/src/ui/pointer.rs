// SPDX-License-Identifier: GPL-3.0-only
use crate::geometry::{Axis, Point, Size};
use crate::view::InputTime;
use std::cell::RefCell;
use std::hash::Hash;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PointerId {
    Primary,
    Touch(TouchId),
}

impl PointerId {
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
    pub(crate) fn drags_content(self) -> bool {
        !matches!(self, Self::Mouse)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TouchId {
    pub seat: i64,
    pub point: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerPhase {
    Pressed,
    Moved,
    Released,
    Cancelled,
}

#[derive(Clone, Copy)]
pub struct PointerInput {
    pub position: Point,
    pub size: Size,
    pub phase: PointerPhase,
    pub pointer: PointerId,
    pub time: Option<InputTime>,
}

pub type PointerAction = Rc<RefCell<dyn FnMut(PointerInput)>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragPhase {
    Started,
    Moved,
    Ended,
    Cancelled,
}

#[derive(Clone, Copy, Debug)]
pub struct DragInput {
    pub position: Point,
    pub size: Size,
    pub phase: DragPhase,
    pub pointer: PointerId,
    pub time: Option<InputTime>,
}

pub type DragHandler = Rc<RefCell<dyn FnMut(DragInput)>>;

#[derive(Clone)]
pub struct Draggable {
    pub axis: Axis,
    pub handler: DragHandler,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransformInput {
    pub focal: Point,
    pub scale: f64,
    pub phase: DragPhase,
}

pub type TransformHandler = Rc<RefCell<dyn FnMut(TransformInput)>>;
