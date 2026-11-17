// SPDX-License-Identifier: GPL-3.0-only
use super::{
    AccessibilityHandler, Action, ActivateAction, Activation, Align, ContentSizes,
    ContentSizesAction, DragHandler, DragInput, Draggable, Flow, Gesture, KeyAction, MeasureAction,
    Modifier, ModifierElement, ModifierValue, Node, NodeKind, NodeRef, NodeTag, PointerAction,
    PointerInput, ScrollAction, SemanticsProvider, Tooltip, TransformHandler, TransformInput, Ui,
    ViewTag, Weight,
};
use crate::accessibility::{ActionKind, Semantics};
use crate::geometry::{Axis, Color, Size};
use crate::keyboard::Shortcut;
use crate::preferences::TEXT_SCALING;
use crate::reactive::State;
use crate::scene::{FontWeight, TextStyle};
use crate::style::{BorderStyle, InteractionState, StateStyles};
use crate::units::{Dp, NonNegativeDp, NumberError, Positive, Sp};
use crate::view::{CompositionError, Transition};
use std::cell::RefCell;
use std::rc::Rc;

pub(super) type SemanticsUpdate = Rc<dyn Fn(&mut Semantics)>;

#[derive(Clone)]
pub(super) enum NodeSetting {
    Measured(MeasureAction),
    ContentSizes(ContentSizesAction),
    OutOfFlow,
    Transition(Transition),
    VisualScale(Positive),
    Gesture(Gesture),
    Key(KeyAction),
    Modal,
    Enabled(bool),
    Selected(bool),
    TrackInteraction(State<InteractionState>),
    StateStyles(StateStyles),
    UserSelect(bool),
    Scroll(ScrollAction),
    Weight(Result<Weight, NumberError>),
    Tag(NodeTag),
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
    pub(super) fn apply(&self, node: &mut Node, ui: &Ui) -> Result<(), CompositionError> {
        match self {
            Self::Measured(action) => node.measured = Some(action.clone()),
            Self::ContentSizes(action) => node.content_sizes = Some(action.clone()),
            Self::OutOfFlow => node.flow = Flow::Out,
            Self::Transition(transition) => node.transition = Some(*transition),
            Self::VisualScale(scale) => node.visual_scale = *scale,
            Self::Gesture(gesture) => node.gestures.set(gesture.clone()),
            Self::Key(action) => node.key_action = Some(action.clone()),
            Self::Modal => node.modal = true,
            Self::Enabled(enabled) => node.enabled = *enabled,
            Self::Selected(selected) => node.selected = Some(*selected),
            Self::TrackInteraction(state) => node.interaction_observer = Some(state.clone()),
            Self::StateStyles(styles) => node.interaction.states = Some(styles.clone()),
            Self::UserSelect(enabled) => node.user_select = Some(*enabled),
            Self::Scroll(action) => node.scroll_action = Some(action.clone()),
            Self::Weight(Ok(weight)) => {
                node.invalid_weight = None;
                let Flow::In(share) = &mut node.flow else {
                    return Err(CompositionError::WeightOutOfFlow);
                };
                *share = Some(*weight);
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
                change_text(node, |current| *current = style)?;
            }
            Self::FontSize(size) => {
                let size = ui.local(&TEXT_SCALING).to_dp(*size);
                change_text(node, |style| style.size = size)?;
            }
            Self::FontFamily(family) => {
                change_text(node, |style| style.family.clone_from(family))?;
            }
            Self::FontWeight(weight) => change_text(node, |style| style.weight = *weight)?,
            Self::MaxLines(lines) => change_text(node, |style| style.max_lines = Some(*lines))?,
            Self::Color(color) => match &mut node.kind {
                NodeKind::Icon { color: icon, .. } => *icon = *color,
                _ => change_text(node, |style| style.color = *color)?,
            },
            Self::Gap(gap) => node.gap = *gap,
            Self::Align(align) => node.align = *align,
        }
        Ok(())
    }
}

fn change_text(
    node: &mut Node,
    change: impl FnOnce(&mut TextStyle<Dp>),
) -> Result<(), CompositionError> {
    let style = node
        .text_style_mut()
        .ok_or(CompositionError::TextStyleWithoutText)?;
    change(style);
    Ok(())
}

impl Modifier {
    pub fn on_measure(self, action: impl FnMut(Size) + 'static) -> Self {
        let action: MeasureAction = Rc::new(RefCell::new(action));
        self.set(NodeSetting::Measured(action))
    }

    pub fn on_content_sizes(self, action: impl FnMut(ContentSizes) + 'static) -> Self {
        let action: ContentSizesAction = Rc::new(RefCell::new(action));
        self.set(NodeSetting::ContentSizes(action))
    }

    pub fn out_of_flow(self) -> Self {
        self.set(NodeSetting::OutOfFlow)
    }

    pub fn transition(self, transition: impl Into<Transition>) -> Self {
        self.set(NodeSetting::Transition(transition.into()))
    }

    pub fn visual_scale(self, scale: Positive) -> Self {
        self.set(NodeSetting::VisualScale(scale))
    }

    pub fn on_click(self, action: impl FnMut() + 'static) -> Self {
        let action: Action = Rc::new(RefCell::new(action));
        self.set(NodeSetting::Gesture(Gesture::Click(action)))
    }

    pub fn on_activate(self, action: impl FnMut(Activation) + 'static) -> Self {
        let action: ActivateAction = Rc::new(RefCell::new(action));
        self.set(NodeSetting::Gesture(Gesture::Activate(action)))
    }

    pub fn on_context(self, action: impl FnMut(Activation) + 'static) -> Self {
        let action: ActivateAction = Rc::new(RefCell::new(action));
        self.set(NodeSetting::Gesture(Gesture::Context(action)))
    }

    pub fn on_key(self, action: impl FnMut(Shortcut) -> bool + 'static) -> Self {
        let action: KeyAction = Rc::new(RefCell::new(action));
        self.set(NodeSetting::Key(action))
    }

    pub fn modal(self) -> Self {
        self.set(NodeSetting::Modal)
    }

    pub fn enabled(self, enabled: bool) -> Self {
        self.set(NodeSetting::Enabled(enabled))
    }

    pub fn selected(self, selected: bool) -> Self {
        self.set(NodeSetting::Selected(selected))
    }

    pub fn track_interaction(self, state: State<InteractionState>) -> Self {
        self.set(NodeSetting::TrackInteraction(state))
    }

    pub fn state_styles(self, styles: StateStyles) -> Self {
        self.set(NodeSetting::StateStyles(styles))
    }

    pub fn user_select(self, enabled: bool) -> Self {
        self.set(NodeSetting::UserSelect(enabled))
    }

    pub fn on_pointer(self, action: impl FnMut(PointerInput) + 'static) -> Self {
        let action: PointerAction = Rc::new(RefCell::new(action));
        self.set(NodeSetting::Gesture(Gesture::Pointer(action)))
    }

    pub fn on_drag(self, axis: Axis, action: impl FnMut(DragInput) + 'static) -> Self {
        let handler: DragHandler = Rc::new(RefCell::new(action));
        self.set(NodeSetting::Gesture(Gesture::Drag(Draggable {
            axis,
            handler,
        })))
    }

    pub fn on_transform(self, action: impl FnMut(TransformInput) + 'static) -> Self {
        let action: TransformHandler = Rc::new(RefCell::new(action));
        self.set(NodeSetting::Gesture(Gesture::Transform(action)))
    }

    pub fn on_scroll(self, action: impl FnMut(Dp) + 'static) -> Self {
        let action: ScrollAction = Rc::new(RefCell::new(action));
        self.set(NodeSetting::Scroll(action))
    }

    pub fn text_style(self, value: TextStyle) -> Self {
        self.set(NodeSetting::TextStyle(value))
    }

    pub fn gap(self, gap: Dp) -> Self {
        match NonNegativeDp::try_new(gap) {
            Ok(gap) => self.set(NodeSetting::Gap(gap.get())),
            Err(cause) => self.push(ModifierElement::Invalid {
                value: ModifierValue::Gap,
                cause,
            }),
        }
    }

    pub fn align_items(self, align: Align) -> Self {
        self.set(NodeSetting::Align(align))
    }

    pub fn weight(self, weight: f64) -> Self {
        self.weighted(weight, Weight::Fill)
    }

    pub fn weight_up_to(self, weight: f64) -> Self {
        self.weighted(weight, Weight::UpTo)
    }

    fn weighted(self, weight: f64, variant: fn(Positive) -> Weight) -> Self {
        self.set(NodeSetting::Weight(Positive::try_new(weight).map(variant)))
    }

    pub fn tag(self, tag: impl ViewTag) -> Self {
        self.set(NodeSetting::Tag(NodeTag::new(tag)))
    }

    pub fn node_ref(self, node: &NodeRef) -> Self {
        self.set(NodeSetting::Reference(node.clone()))
    }

    pub fn focusable(self, focusable: bool) -> Self {
        self.set(NodeSetting::Focusable(focusable))
    }

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
