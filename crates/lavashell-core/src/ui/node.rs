// SPDX-License-Identifier: GPL-3.0-only
use super::{
    Component, Draggable, ModifierElement, NodeRef, PointerAction, Scroller, TransformHandler,
};
use crate::accessibility::{AccessibilityAction, ActionError};
use crate::animation::AnimationError;
use crate::geometry::{Axis, Color, Point, Size};
use crate::input_method::TextHints;
use crate::keyboard::Shortcut;
use crate::preferences::TextScaling;
use crate::protected_text::ProtectedInput;
use crate::reactive::State;
use crate::scene::{DrawContext, Drawing, Icon, TextStyle};
use crate::style::{BorderStyle, InteractionStyle};
use crate::units::{Dp, NumberError, Positive};
use crate::view::Transition;
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::hash::Hash;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct NodeId(pub u64);

pub type Action = Rc<RefCell<dyn FnMut()>>;
pub type TextChange = Rc<RefCell<dyn FnMut(String)>>;
pub(crate) type DrawCallback = Rc<dyn Fn(&mut Drawing<'_>)>;
pub type ScrollAction = Rc<RefCell<dyn FnMut(Dp)>>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct InputModifiers {
    pub control: bool,
    pub shift: bool,
    pub alt: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct Activation {
    pub position: Point,
    pub modifiers: InputModifiers,
}

pub type ActivateAction = Rc<RefCell<dyn FnMut(Activation)>>;
pub type KeyAction = Rc<RefCell<dyn FnMut(Shortcut) -> bool>>;
pub type MeasureAction = Rc<RefCell<dyn FnMut(Size)>>;
pub type ContentSizesAction = Rc<RefCell<dyn FnMut(ContentSizes)>>;

/// `preferred` is the size of a node when no constraint limits it. `minimum` is the smallest size of
/// the node.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ContentSizes {
    pub preferred: Size,
    pub minimum: Size,
}
pub type AccessibilityHandler =
    Rc<RefCell<dyn FnMut(AccessibilityAction) -> Result<(), ActionError>>>;
pub type SemanticsProvider = Rc<dyn Fn(&mut crate::accessibility::Semantics)>;
pub(crate) type Memory = Rc<RefCell<crate::states::States<(NodeId, u64)>>>;

/// How a container places its children: one after another along an axis, or each at the origin
/// of the container over the children before it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Arrangement {
    Along(Axis),
    Overlay,
}

impl From<Axis> for Arrangement {
    fn from(axis: Axis) -> Self {
        Self::Along(axis)
    }
}

/// The share of the free length along a stack that goes to a child, in proportion to the factor of
/// the weight.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Weight {
    Fill(Positive),
    UpTo(Positive),
}

impl Weight {
    pub(crate) fn factor(self) -> f64 {
        match self {
            Self::Fill(factor) | Self::UpTo(factor) => factor.get(),
        }
    }
}

/// How a node takes part in the layout of its parent.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Flow {
    /// Among its siblings, with the share of the free length given by its weight, if it has one.
    In(Option<Weight>),
    /// At the origin of the parent, without contributing to its size or gaps.
    Out,
}

/// Where the ID of a node is unique. A move within that scope preserves the state of the node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Identity {
    /// Among the children of its parent.
    InParent,
    /// In the whole view, for a node composed with [`Ui::shared`](super::Ui::shared).
    InView,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Align {
    #[default]
    Start,
    Center,
    End,
}

/// The side length of the square in which an icon is drawn.
#[derive(Clone, Debug, PartialEq)]
pub enum IconSize {
    /// As tall as one line of text in the style.
    Line(TextStyle<Dp>),
    Fixed(Dp),
}

/// How the paths of an icon are drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum IconStyle {
    /// Stroked paths are drawn as lines.
    #[default]
    Outlined,
    /// Closed stroked paths are filled as well.
    Filled,
}

/// The size and the style with which an icon is drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct IconOptions {
    /// The side length of the square of the icon. `None` makes the side as long as one line of the
    /// text style in effect around the icon is tall.
    pub size: Option<Dp>,
    pub style: IconStyle,
}

/// The placeholder and the input method hints of a text field.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextFieldOptions {
    /// The text shown while the field is empty.
    pub placeholder: String,
    /// Hints for the input method about the field. In a single-line field, an Enter key that no
    /// `on_key` handler consumes moves the focus to the next field when the action of the hints is
    /// `Next`. It clears the focus when the action is `Done`.
    pub hints: TextHints,
}

#[derive(Clone)]
pub enum NodeKind {
    Container(Arrangement),
    Text(String, TextStyle<Dp>),
    Spacer(Size),
    Canvas {
        size: Size,
        draw: DrawCallback,
        draw_context: DrawContext,
    },
    Icon {
        icon: &'static Icon,
        color: Color,
        style: IconStyle,
        size: IconSize,
    },
    TextInput {
        content: FieldContent,
        placeholder: String,
        style: TextStyle<Dp>,
        hints: TextHints,
    },
}

/// What a field edits.
#[derive(Clone)]
pub enum FieldContent {
    /// Text shown unchanged. Every edit of the text is passed to `change`.
    Plain {
        value: String,
        change: TextChange,
        lines: Lines,
    },
    /// Secret text that the field shows masked. The field never passes the plain text on.
    Protected(ProtectedInput),
}

/// How many lines a field shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lines {
    /// One line, which scrolls sideways.
    One,
    /// The lines that the text wraps to at the width of the field, which scroll up and down.
    Several,
}

impl FieldContent {
    /// The text that the field shows: its value, or one mask for each character of protected text.
    pub(crate) fn shown(&self) -> Cow<'_, str> {
        match self {
            Self::Plain { value, .. } => Cow::Borrowed(value),
            Self::Protected(input) => Cow::Owned(input.masked()),
        }
    }

    pub(crate) fn lines(&self) -> Lines {
        match self {
            Self::Plain { lines, .. } => *lines,
            Self::Protected(_) => Lines::One,
        }
    }

    pub(crate) fn protected(&self) -> Option<&ProtectedInput> {
        match self {
            Self::Plain { .. } => None,
            Self::Protected(input) => Some(input),
        }
    }

    /// `style` with `max_lines` set to 1 for a single-line field, or unset for a field with several
    /// lines.
    pub(crate) fn line_style(&self, style: &TextStyle<Dp>) -> TextStyle<Dp> {
        let max_lines = match self.lines() {
            Lines::One => Some(1),
            Lines::Several => None,
        };
        TextStyle {
            max_lines,
            ..style.clone()
        }
    }
}

#[derive(Clone)]
pub struct Node {
    pub id: NodeId,
    pub kind: NodeKind,
    pub children: Vec<usize>,
    pub modifier: Vec<ModifierElement>,
    pub gap: Dp,
    pub align: Align,
    pub flow: Flow,
    pub(crate) invalid_weight: Option<NumberError>,
    pub visual_scale: Positive,
    pub(super) scroll: Option<Scroller>,
    pub activate: Option<ActivateAction>,
    pub context: Option<ActivateAction>,
    pub key_action: Option<KeyAction>,
    pub measured: Option<MeasureAction>,
    pub content_sizes: Option<ContentSizesAction>,
    pub modal: bool,
    pub focusable: bool,
    pub focus_within: Option<BorderStyle>,
    pub tag: Option<String>,
    pub(crate) references: Vec<NodeRef>,
    pub tooltip: Option<Tooltip>,
    pub interaction: InteractionStyle,
    pub interaction_observer: Option<State<crate::style::InteractionState>>,
    /// Whether the user can select the text of the node. `None` uses the setting of the nearest
    /// ancestor that has one.
    pub user_select: Option<bool>,
    pub action: Option<Action>,
    pub pointer: Option<PointerAction>,
    pub drag: Option<Draggable>,
    pub transform: Option<TransformHandler>,
    pub scroll_action: Option<ScrollAction>,
    pub enabled: bool,
    pub selected: Option<bool>,
    pub transition: Option<Transition>,
    pub identity: Identity,
    pub semantics: crate::accessibility::Semantics,
    pub semantics_provider: Option<SemanticsProvider>,
    pub accessibility_actions: BTreeMap<crate::accessibility::ActionKind, AccessibilityHandler>,
    pub(crate) component: Option<Rc<Component>>,
    pub(crate) on_exit: Vec<Rc<dyn Fn() -> Result<(), AnimationError>>>,
}

/// The label of a tooltip, with the text scaling in effect where the node that shows the tooltip was
/// composed.
#[derive(Clone)]
pub struct Tooltip {
    pub label: String,
    pub text_scaling: TextScaling,
}

impl Node {
    pub(crate) fn new(id: NodeId, kind: NodeKind, interaction: InteractionStyle) -> Self {
        Self {
            id,
            kind,
            children: Vec::new(),
            modifier: Vec::new(),
            gap: Dp::ZERO,
            align: Align::Start,
            flow: Flow::In(None),
            invalid_weight: None,
            visual_scale: crate::constant!(Positive::try_new(1.0)),
            scroll: None,
            activate: None,
            context: None,
            key_action: None,
            measured: None,
            content_sizes: None,
            modal: false,
            focusable: true,
            focus_within: None,
            tag: None,
            references: Vec::new(),
            tooltip: None,
            interaction,
            interaction_observer: None,
            user_select: None,
            action: None,
            pointer: None,
            drag: None,
            transform: None,
            scroll_action: None,
            enabled: true,
            selected: None,
            transition: None,
            identity: Identity::InParent,
            semantics: crate::accessibility::Semantics::default(),
            semantics_provider: None,
            accessibility_actions: BTreeMap::new(),
            component: None,
            on_exit: Vec::new(),
        }
    }

    /// Whether the node declares a handler of pointer, key or wheel input, or scrolls.
    pub(crate) fn handles_input(&self) -> bool {
        self.action.is_some()
            || self.activate.is_some()
            || self.context.is_some()
            || self.pointer.is_some()
            || self.drag.is_some()
            || self.transform.is_some()
            || self.scroll_action.is_some()
            || self.key_action.is_some()
            || self.scroll_axis().is_some()
    }

    /// Whether the accessibility tree contains the node even when its semantics are empty.
    pub(crate) fn exports_state(&self) -> bool {
        self.selected.is_some()
    }

    /// The `on_scroll` handler of the node for a wheel turned along `axis`. `None` for a horizontal
    /// wheel, because the handler is called only for vertical wheel input.
    pub(crate) fn wheel_handler(&self, axis: Axis) -> Option<&ScrollAction> {
        self.scroll_action
            .as_ref()
            .filter(|_| axis == Axis::Vertical)
    }

    /// The text style of a text or a field.
    pub(super) fn text_style_mut(&mut self) -> Option<&mut TextStyle<Dp>> {
        match &mut self.kind {
            NodeKind::Text(_, style) | NodeKind::TextInput { style, .. } => Some(style),
            _ => None,
        }
    }

    /// Replaces the children and the exit actions of this node with those of `root`, the root node
    /// built by a component, and leaves `root` without them.
    pub(crate) fn take_contents(&mut self, root: &mut Node) {
        self.children = std::mem::take(&mut root.children);
        self.on_exit = std::mem::take(&mut root.on_exit);
    }

    /// Removes the children and the exit actions of this node.
    pub(crate) fn clear_contents(&mut self) {
        self.children.clear();
        self.on_exit.clear();
    }
}
