// SPDX-License-Identifier: GPL-3.0-only

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

#[derive(Clone, Debug, Default, PartialEq)]
pub struct InteractionStyle {
    pub states: Option<StateStyles>,
    pub focus: Option<BorderStyle>,
    pub placeholder: Option<Color>,
    pub tooltip: Option<TooltipStyle>,
    pub caret: Option<Color>,
    pub selection: Option<Color>,
}

pub static INTERACTION_STYLE: Local<InteractionStyle> = Local::new(InteractionStyle::default);
