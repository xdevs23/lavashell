// SPDX-License-Identifier: GPL-3.0-only
use super::{
    AccessibilityHandler, Action, ActivateAction, Activation, Align, ContentSizes,
    ContentSizesAction, DragHandler, DragInput, Draggable, Flow, KeyAction, MeasureAction,
    Modifier, Node, NodeKind, NodeRef, PointerAction, PointerInput, ScrollAction,
    SemanticsProvider, Tooltip, TransformHandler, TransformInput, Ui, Weight,
};
use crate::accessibility::{ActionKind, Semantics};
use crate::geometry::{Axis, Color, Size};
use crate::keyboard::Shortcut;
use crate::preferences::TEXT_SCALING;
use crate::reactive::State;
use crate::scene::{FontWeight, TextStyle};
use crate::style::{BorderStyle, InteractionState, StateStyles};
use crate::units::{Dp, NumberError, Positive, Sp};
use crate::view::Transition;
use std::cell::RefCell;
use std::rc::Rc;

/// A change of the semantics of a node.
pub(super) type SemanticsUpdate = Rc<dyn Fn(&mut Semantics)>;

/// A setting that a modifier applies to a node when the node is added. The text settings change
/// only a node that shows text, and the color also changes an icon. The gap and the alignment
/// change only a node that places its children along an axis. Every other setting applies to every
/// kind of node.
#[derive(Clone)]
pub(super) enum NodeSetting {
    Measured(MeasureAction),
    ContentSizes(ContentSizesAction),
    OutOfFlow,
    Transition(Transition),
    VisualScale(Positive),
    Click(Action),
    Activate(ActivateAction),
    Context(ActivateAction),
    Key(KeyAction),
    Modal,
    Enabled(bool),
    Selected(bool),
    TrackInteraction(State<InteractionState>),
    StateStyles(StateStyles),
    UserSelect(bool),
    Pointer(PointerAction),
    Drag(Draggable),
    Transform(TransformHandler),
    Scroll(ScrollAction),
    Weight(Result<Option<Weight>, NumberError>),
    Tag(String),
    Reference(NodeRef),
    Focusable(bool),
    FocusWithin(BorderStyle),
    Tooltip(String),
    Semantics(SemanticsUpdate),
    SemanticsProvider(SemanticsProvider),
    AccessibilityAction(ActionKind, AccessibilityHandler),
    TextStyle(TextStyle),
    FontSize(Sp),
    FontFamily(String),
    FontWeight(FontWeight),
    MaxLines(usize),
    Color(Color),
    Gap(Dp),
    Align(Align),
}

impl NodeSetting {
    /// Applies this setting to `node`. Settings that depend on locals, such as the text scaling, read
    /// them from `ui`, the composition in which the node is added.
    pub(super) fn apply(&self, node: &mut Node, ui: &Ui) {
        match self {
            Self::Measured(action) => node.measured = Some(action.clone()),
            Self::ContentSizes(action) => node.content_sizes = Some(action.clone()),
            Self::OutOfFlow => node.flow = Flow::Out,
            Self::Transition(transition) => node.transition = Some(*transition),
            Self::VisualScale(scale) => node.visual_scale = *scale,
            Self::Click(action) => node.action = Some(action.clone()),
            Self::Activate(action) => node.activate = Some(action.clone()),
            Self::Context(action) => node.context = Some(action.clone()),
            Self::Key(action) => node.key_action = Some(action.clone()),
            Self::Modal => node.modal = true,
            Self::Enabled(enabled) => node.enabled = *enabled,
            Self::Selected(selected) => node.selected = Some(*selected),
            Self::TrackInteraction(state) => node.interaction_observer = Some(state.clone()),
            Self::StateStyles(styles) => node.interaction.states = Some(styles.clone()),
            Self::UserSelect(enabled) => node.user_select = Some(*enabled),
            Self::Pointer(action) => node.pointer = Some(action.clone()),
            Self::Drag(draggable) => node.drag = Some(draggable.clone()),
            Self::Transform(action) => node.transform = Some(action.clone()),
            Self::Scroll(action) => node.scroll_action = Some(action.clone()),
            Self::Weight(Ok(weight)) => {
                node.invalid_weight = None;
                if let Flow::In(share) = &mut node.flow {
                    *share = *weight;
                }
            }
            Self::Weight(Err(error)) => node.invalid_weight = Some(*error),
            Self::Tag(tag) => node.tag = Some(tag.clone()),
            Self::Reference(reference) => node.references.push(reference.clone()),
            Self::Focusable(focusable) => node.focusable = *focusable,
            Self::FocusWithin(border) => node.focus_within = Some(border.clone()),
            Self::Tooltip(label) => {
                node.tooltip = Some(Tooltip {
                    label: label.clone(),
                    text_scaling: ui.local(&TEXT_SCALING),
                });
            }
            Self::Semantics(update) => update(&mut node.semantics),
            Self::SemanticsProvider(provider) => node.semantics_provider = Some(provider.clone()),
            Self::AccessibilityAction(kind, action) => {
                let _handler_of_an_earlier_modifier =
                    node.accessibility_actions.insert(*kind, action.clone());
            }
            Self::TextStyle(style) => {
                let style = ui.resolve_text_style(style.clone());
                change_text(node, |current| *current = style);
            }
            Self::FontSize(size) => {
                let size = ui.local(&TEXT_SCALING).to_dp(*size);
                change_text(node, |style| style.size = size);
            }
            Self::FontFamily(family) => change_text(node, |style| style.family.clone_from(family)),
            Self::FontWeight(weight) => change_text(node, |style| style.weight = *weight),
            Self::MaxLines(lines) => change_text(node, |style| style.max_lines = Some(*lines)),
            Self::Color(color) => match &mut node.kind {
                NodeKind::Icon { color: icon, .. } => *icon = *color,
                _ => change_text(node, |style| style.color = *color),
            },
            Self::Gap(gap) => node.gap = *gap,
            Self::Align(align) => node.align = *align,
        }
    }
}

/// Changes the text style of `node` with `change` when the node shows text.
fn change_text(node: &mut Node, change: impl FnOnce(&mut TextStyle<Dp>)) {
    if let Some(style) = node.text_style_mut() {
        change(style);
    }
}

impl Modifier {
    /// Calls `action` with the measured outer size of the node after layout. Set reactive state from
    /// `action` only when the value changed, because a reactive update can request another layout
    /// pass.
    pub fn on_measure(self, action: impl FnMut(Size) + 'static) -> Self {
        let action: MeasureAction = Rc::new(RefCell::new(action));
        self.set(NodeSetting::Measured(action))
    }

    /// Calls `action` with the content sizes of the node, measured in each layout of a frame: the
    /// size without a constraint, and the smallest size. Set reactive state from `action` only when
    /// the value changed, because a reactive update can request another layout pass.
    pub fn on_content_sizes(self, action: impl FnMut(ContentSizes) + 'static) -> Self {
        let action: ContentSizesAction = Rc::new(RefCell::new(action));
        self.set(NodeSetting::ContentSizes(action))
    }

    /// Takes the node out of the flow of its parent. The node is measured and painted at the origin
    /// of the parent and does not count toward the size or the gaps of the parent. The clips and the
    /// input handlers set on this node and its ancestors still apply.
    pub fn out_of_flow(self) -> Self {
        self.set(NodeSetting::OutOfFlow)
    }

    pub fn transition(self, transition: impl Into<Transition>) -> Self {
        self.set(NodeSetting::Transition(transition.into()))
    }

    /// Scales the laid-out subtree of the node about its center, without changing the layout.
    /// Painting, hit testing and accessibility all use the scaled geometry.
    pub fn visual_scale(self, scale: Positive) -> Self {
        self.set(NodeSetting::VisualScale(scale))
    }

    /// Calls `action` when the node is clicked. Unlike a visual modifier element, the position of
    /// this setting in the modifier does not matter.
    pub fn on_click(self, action: impl FnMut() + 'static) -> Self {
        let action: Action = Rc::new(RefCell::new(action));
        self.set(NodeSetting::Click(action))
    }

    pub fn on_activate(self, action: impl FnMut(Activation) + 'static) -> Self {
        let action: ActivateAction = Rc::new(RefCell::new(action));
        self.set(NodeSetting::Activate(action))
    }

    pub fn on_context(self, action: impl FnMut(Activation) + 'static) -> Self {
        let action: ActivateAction = Rc::new(RefCell::new(action));
        self.set(NodeSetting::Context(action))
    }

    /// Calls `action` with each key pressed while this node or one of its descendants has the focus.
    /// `action` returns `true` when it consumed the key.
    pub fn on_key(self, action: impl FnMut(Shortcut) -> bool + 'static) -> Self {
        let action: KeyAction = Rc::new(RefCell::new(action));
        self.set(NodeSetting::Key(action))
    }

    /// Makes the node modal. The nodes composed before it no longer get pointer hits or the keyboard
    /// focus.
    pub fn modal(self) -> Self {
        self.set(NodeSetting::Modal)
    }

    pub fn enabled(self, enabled: bool) -> Self {
        self.set(NodeSetting::Enabled(enabled))
    }

    /// Marks the node as selected or not selected. Assistive technology announces this state. The
    /// state passed to state styles has it in `InteractionState::selected`.
    pub fn selected(self, selected: bool) -> Self {
        self.set(NodeSetting::Selected(selected))
    }

    /// Writes the `InteractionState` passed to the state styles of the node into `state`. `state` is
    /// set only when the interaction state changes, also when a press is cancelled or the node
    /// becomes disabled.
    pub fn track_interaction(self, state: State<InteractionState>) -> Self {
        self.set(NodeSetting::TrackInteraction(state))
    }

    /// Sets the state styles of the node in place of the inherited ones, without changing how the
    /// control behaves.
    pub fn state_styles(self, styles: StateStyles) -> Self {
        self.set(NodeSetting::StateStyles(styles))
    }

    /// Allows or prevents the selection of plain text in this node and its descendants. Text fields
    /// are not affected.
    pub fn user_select(self, enabled: bool) -> Self {
        self.set(NodeSetting::UserSelect(enabled))
    }

    /// Calls `action` with every change of a pointer whose press hits this node, before any node
    /// inside it. The node captures the pointer at the press, so a scrolling ancestor does not
    /// scroll with that pointer.
    pub fn on_pointer(self, action: impl FnMut(PointerInput) + 'static) -> Self {
        let action: PointerAction = Rc::new(RefCell::new(action));
        self.set(NodeSetting::Pointer(action))
    }

    /// Calls `action` with a drag along `axis` of a pointer pressed on this node or inside it. The
    /// drag starts once it claims the pointer. That happens at the press when no other gesture
    /// recognizer competes for the pointer. Otherwise it happens when the pointer moves past the slop
    /// distance along `axis` before another recognizer accepts the pointer.
    pub fn on_drag(self, axis: Axis, action: impl FnMut(DragInput) + 'static) -> Self {
        let handler: DragHandler = Rc::new(RefCell::new(action));
        self.set(NodeSetting::Drag(Draggable { axis, handler }))
    }

    /// Calls `action` with the pinch of the first two touch points pressed on this node or inside
    /// it. The pinch starts when the node has claimed both touch points. `action` is then called for
    /// each change. While a handler inside this node holds a touch point, the node takes both points
    /// once the distance between them changed by more than the touch slop, and the `on_pointer`
    /// handlers there receive `PointerPhase::Cancelled`.
    pub fn on_transform(self, action: impl FnMut(TransformInput) + 'static) -> Self {
        let action: TransformHandler = Rc::new(RefCell::new(action));
        self.set(NodeSetting::Transform(action))
    }

    /// Calls `action` with vertical wheel and trackpad input over this node or its descendants and
    /// consumes that input. A positive delta moves down, as the vertical delta of
    /// `InputEvent::Scroll` does. A delta of zero or one that is not finite is ignored.
    pub fn on_scroll(self, action: impl FnMut(Dp) + 'static) -> Self {
        let action: ScrollAction = Rc::new(RefCell::new(action));
        self.set(NodeSetting::Scroll(action))
    }

    pub fn text_style(self, value: TextStyle) -> Self {
        self.set(NodeSetting::TextStyle(value))
    }

    pub fn gap(self, gap: Dp) -> Self {
        self.set(NodeSetting::Gap(gap.max(Dp::ZERO)))
    }

    pub fn align_items(self, align: Align) -> Self {
        self.set(NodeSetting::Align(align))
    }

    /// A weight of zero leaves the node unweighted. A negative or non-finite weight fails the
    /// layout of the view.
    pub fn weight(self, weight: f64) -> Self {
        self.weighted(weight, Weight::Fill)
    }

    /// Gives the node at most the share of the free length set by `weight`, and no more than the
    /// content of the node needs. The rest of the share is left empty.
    pub fn weight_up_to(self, weight: f64) -> Self {
        self.weighted(weight, Weight::UpTo)
    }

    fn weighted(self, weight: f64, variant: fn(Positive) -> Weight) -> Self {
        let weight = (weight != 0.0).then(|| Positive::try_new(weight).map(variant));
        self.set(NodeSetting::Weight(weight.transpose()))
    }

    pub fn tag(self, tag: impl Into<String>) -> Self {
        self.set(NodeSetting::Tag(tag.into()))
    }

    /// Makes `node` refer to this node. References set on the node before refer to it as well.
    pub fn node_ref(self, node: &NodeRef) -> Self {
        self.set(NodeSetting::Reference(node.clone()))
    }

    /// Sets whether the node can get the keyboard focus. A node that is not focusable still handles
    /// pointer input.
    pub fn focusable(self, focusable: bool) -> Self {
        self.set(NodeSetting::Focusable(focusable))
    }

    /// Draws `border` around the padded bounds of this node while the node or a descendant shows its
    /// focus. The node does not become an input target or a tab stop because of this.
    pub fn focus_within(self, border: BorderStyle) -> Self {
        self.set(NodeSetting::FocusWithin(border))
    }

    pub fn tooltip(self, label: impl Into<String>) -> Self {
        self.set(NodeSetting::Tooltip(label.into()))
    }

    pub fn font_size(self, size: Sp) -> Self {
        self.set(NodeSetting::FontSize(size))
    }

    pub fn font_family(self, family: impl Into<String>) -> Self {
        self.set(NodeSetting::FontFamily(family.into()))
    }

    pub fn font_weight(self, weight: FontWeight) -> Self {
        self.set(NodeSetting::FontWeight(weight))
    }

    pub fn max_lines(self, lines: usize) -> Self {
        self.set(NodeSetting::MaxLines(lines))
    }

    pub fn color(self, color: Color) -> Self {
        self.set(NodeSetting::Color(color))
    }
}
