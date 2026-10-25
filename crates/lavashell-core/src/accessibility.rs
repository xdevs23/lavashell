// SPDX-License-Identifier: GPL-3.0-only
//! Snapshots of the accessibility tree of a view, and the actions that assistive technology
//! requests on its nodes. Both are independent of the renderer.
//!
//! Text offsets count Unicode scalar values. Reading the tree does not move the keyboard focus.
//! Only a `Focus` action moves it.

mod action_kinds;
mod actions;
mod semantics;
mod tree;

pub use action_kinds::ActionKinds;
pub use actions::*;
pub use semantics::*;
pub use tree::*;
