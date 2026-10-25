// SPDX-License-Identifier: GPL-3.0-only
//! Styles for the borders, tooltips and interaction states of nodes. An application sets them through the
//! [`INTERACTION_STYLE`] local, and every node composed inside inherits them.

use crate::effects::{BorderPlacement, Stroke};
use crate::geometry::{Color, Insets};
use crate::local::Local;
use crate::scene::TextStyle;
use crate::units::{Dp, NonNegativeDp};

mod states;

pub(crate) use states::{AnimatedAppearance, ContentColors};
pub use states::{FocusState, InteractionAppearance, InteractionState, PointerState, StateStyles};

#[derive(Clone, Debug, PartialEq)]
pub struct BorderStyle {
    pub color: Color,
    pub width: NonNegativeDp,
    pub radius: Dp,
}

impl BorderStyle {
    pub const NONE: Self = Self {
        color: Color::TRANSPARENT,
        width: NonNegativeDp::ZERO,
        radius: Dp::ZERO,
    };

    /// The stroke that paints the border inside the outline of the node.
    pub(crate) const fn stroke(&self) -> Stroke {
        Stroke {
            color: self.color,
            width: self.width,
            placement: BorderPlacement::Inside,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TooltipStyle {
    pub text: TextStyle,
    pub background: Color,
    pub border: BorderStyle,
    pub padding: Insets,
    pub gap: Dp,
}

/// The colors and borders with which controls paint their states, focus ring, placeholder, tooltip, caret and
/// selection. Every field is `None` by default. Controls then paint none of these decorations.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct InteractionStyle {
    pub states: Option<StateStyles>,
    /// The focus ring of a node whose focus is visible and whose appearance from `states` paints no outline.
    pub focus: Option<BorderStyle>,
    pub placeholder: Option<Color>,
    pub tooltip: Option<TooltipStyle>,
    pub caret: Option<Color>,
    pub selection: Option<Color>,
}

pub static INTERACTION_STYLE: Local<InteractionStyle> = Local::new(InteractionStyle::default);
