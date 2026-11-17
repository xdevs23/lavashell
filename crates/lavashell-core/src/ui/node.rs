// SPDX-License-Identifier: GPL-3.0-only
use super::{Component, Gestures, ModifierElement, NodeRef, NodeTag, Scroller, ViewTag};
use crate::accessibility::{AccessibilityAction, ActionError};
use crate::animation::AnimationError;
use crate::geometry::{Axis, Color, Point, Size};
use crate::input_method::TextHints;
use crate::keyboard::Shortcut;
use crate::preferences::TextScaling;
use crate::protected_text::ProtectedInput;
use crate::reactive::State;
use crate::scene::{DrawContext, Drawing, Icon, IconStyle, TextStyle};
use crate::style::{BorderStyle, InteractionStyle};
use crate::units::{Dp, NumberError, Positive};
use crate::view::{Crossing, Transition};
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct NodeId(pub u64);

#[derive(Hash)]
enum IdScope {
    Position,
    Keyed,
    Shared,
}

impl NodeId {
    pub const ROOT: Self = Self(1);

    pub(crate) fn at_position(parent: NodeId, index: usize) -> Self {
        Self(hash_of((IdScope::Position, parent, index)))
    }

    pub(crate) fn keyed(parent: NodeId, key: impl Hash) -> Self {
        Self(hash_of((IdScope::Keyed, parent, key)))
    }

    pub(crate) fn shared(view: ViewId, key: impl Hash) -> Self {
        Self(hash_of((IdScope::Shared, view, key)))
    }
}

pub(super) fn hash_of(value: impl Hash) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

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

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ContentSizes {
    pub preferred: Size,
    pub minimum: Size,
}
pub type AccessibilityHandler =
    Rc<RefCell<dyn FnMut(AccessibilityAction) -> Result<(), ActionError>>>;
pub type SemanticsProvider = Rc<dyn Fn(&mut crate::accessibility::Semantics)>;
pub(crate) type Memory = Rc<RefCell<crate::states::States<(NodeId, u64)>>>;

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

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Flow {
    In(Option<Weight>),
    Out,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SharedKey(u64);

impl SharedKey {
    pub(crate) fn of(key: impl Hash) -> Self {
        Self(hash_of(key))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ViewId(pub(crate) NodeId);

impl ViewId {
    pub(crate) const ROOT: Self = Self(NodeId::ROOT);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Identity {
    InParent,
    Shared { view: ViewId, key: SharedKey },
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Align {
    #[default]
    Start,
    Center,
    End,
}

#[derive(Clone, Debug, PartialEq)]
pub enum IconSize {
    Line(TextStyle<Dp>),
    Fixed(Dp),
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct IconOptions {
    pub size: Option<Dp>,
    pub style: IconStyle,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextFieldOptions {
    pub placeholder: String,
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

#[derive(Clone)]
pub enum FieldContent {
    Plain {
        value: String,
        change: TextChange,
        lines: Lines,
    },
    Protected(ProtectedInput),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lines {
    One,
    Several,
}

impl FieldContent {
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
    pub(crate) gestures: Gestures,
    pub key_action: Option<KeyAction>,
    pub measured: Option<MeasureAction>,
    pub content_sizes: Option<ContentSizesAction>,
    pub modal: bool,
    pub focusable: bool,
    pub focus_within: Option<BorderStyle>,
    pub tag: Option<NodeTag>,
    pub(crate) references: Vec<NodeRef>,
    pub tooltip: Option<Tooltip>,
    pub interaction: InteractionStyle,
    pub interaction_observer: Option<State<crate::style::InteractionState>>,
    pub user_select: Option<bool>,
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
    pub(crate) crossings: Vec<Crossing>,
}

#[derive(Clone, PartialEq)]
pub struct Tooltip {
    pub label: String,
    pub text_scaling: TextScaling,
}

impl Node {
    pub fn has_tag<T: ViewTag>(&self, tag: &T) -> bool {
        self.tag.as_ref().is_some_and(|own| own.is(tag))
    }

    pub(crate) fn component_is_dirty(&self) -> bool {
        (self.component.as_ref()).is_some_and(|component| component.scope.is_dirty())
    }

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
            gestures: Gestures::default(),
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
            crossings: Vec::new(),
        }
    }

    pub(crate) fn handles_input(&self) -> bool {
        !self.gestures.is_empty()
            || self.scroll_action.is_some()
            || self.key_action.is_some()
            || self.scroll_axis().is_some()
    }

    pub(crate) fn exports_state(&self) -> bool {
        self.selected.is_some()
    }

    pub(crate) fn wheel_handler(&self, axis: Axis) -> Option<&ScrollAction> {
        self.scroll_action
            .as_ref()
            .filter(|_| axis == Axis::Vertical)
    }

    pub(crate) fn text_style(&self) -> Option<&TextStyle<Dp>> {
        match &self.kind {
            NodeKind::Text(_, style) => Some(style),
            _ => None,
        }
    }

    pub(super) fn text_style_mut(&mut self) -> Option<&mut TextStyle<Dp>> {
        match &mut self.kind {
            NodeKind::Text(_, style) | NodeKind::TextInput { style, .. } => Some(style),
            _ => None,
        }
    }

    pub(crate) fn take_contents(&mut self, root: &mut Node) {
        self.children = std::mem::take(&mut root.children);
        self.on_exit = std::mem::take(&mut root.on_exit);
        self.crossings = std::mem::take(&mut root.crossings);
    }

    pub(crate) fn clear_contents(&mut self) {
        self.children.clear();
        self.on_exit.clear();
        self.crossings.clear();
    }
}
