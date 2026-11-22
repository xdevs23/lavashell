// SPDX-License-Identifier: GPL-3.0-only
mod accessibility;
mod animate;
mod component;
mod deferred;
mod gesture;
mod modifier;
mod node;
mod pointer;
mod rebuilt;
mod scrolling;
mod settings;
mod view_tag;

pub use crate::scene::IconStyle;
use component::Component;
pub(crate) use deferred::laid_out::{Measured, Placed};
pub use deferred::{Bounds, Deferred, NodeRef, Reach, Sizes};
pub(crate) use deferred::{NodeRefKey, Part, Read, Used};
pub(crate) use gesture::{Gesture, Gestures};
pub use modifier::{AnimatedColor, Modifier, ModifierElement, ModifierValue};
pub(crate) use modifier::{LayoutValue, MainLength};
pub(crate) use node::Memory;
pub use node::{
    AccessibilityHandler, Action, ActivateAction, Activation, Align, Arrangement, ContentSizes,
    ContentSizesAction, ContextAction, ContextRequest, FieldContent, Flow, IconOptions, IconSize,
    Identity, InputModifiers, KeyAction, Lines, MeasureAction, Node, NodeId, NodeKind, Overflow,
    ScrollAction, SemanticsProvider, SharedKey, TextChange, TextFieldOptions, Tooltip, ViewId,
    Weight,
};
pub use pointer::{
    DragHandler, DragInput, DragPhase, Draggable, PointerAction, PointerId, PointerInput,
    PointerKind, PointerPhase, TouchId, TransformHandler, TransformInput,
};
pub(crate) use scrolling::Scroller;
pub use view_tag::{NodeTag, ViewTag};

use crate::animation::{Animation, Motion};
use crate::geometry::{Size, VIEWPORT, Viewport};
use crate::input_method::TextSender;
use crate::keyboard::Shortcut;
use crate::local::{Local, LocalReader};
use crate::platform::PASSWORD_MASK;
use crate::preferences::TEXT_SCALING;
use crate::protected_text::ProtectedInput;
use crate::reactive::State;
use crate::scene::{DrawContext, Drawing, Icon, TEXT_STYLE, TextStyle};
use crate::style::{INTERACTION_STYLE, InteractionStyle};
use crate::units::Dp;
use crate::view::{ComponentError, ComponentFailure, CompositionError, Crossing, NodeName};
use std::cell::RefCell;
use std::hash::Hash;
use std::rc::Rc;

static ENCLOSING_VIEW: Local<ViewId> = Local::new(|| ViewId::ROOT);

#[derive(Clone)]
pub(crate) struct ViewHandles {
    pub memory: Memory,
    pub motion: Motion,
    pub text_sender: TextSender,
}

impl Default for ViewHandles {
    fn default() -> Self {
        Self {
            memory: Memory::default(),
            motion: Motion::default(),
            text_sender: TextSender::new(),
        }
    }
}

#[derive(Clone)]
pub(crate) struct ComposedShortcut {
    pub node: NodeId,
    pub shortcut: Shortcut,
    pub action: Action,
}

pub struct Ui {
    pub(crate) nodes: Vec<Node>,
    parent: usize,
    pub(crate) memory: Memory,
    locals: LocalReader,
    pub(crate) shortcuts: Vec<ComposedShortcut>,
    pub(crate) motion: Motion,
    pub(crate) text_sender: TextSender,
    failure: Option<CompositionError>,
    pub(crate) failures: Vec<ComponentFailure>,
}

impl Ui {
    pub(crate) fn composing(handles: ViewHandles, locals: LocalReader) -> Self {
        let ViewHandles {
            memory,
            motion,
            text_sender,
        } = handles;
        Self {
            nodes: vec![Node::new(
                NodeId::ROOT,
                NodeKind::Container(Arrangement::Overlay),
                InteractionStyle::default(),
            )],
            parent: 0,
            memory,
            locals,
            shortcuts: Vec::new(),
            motion,
            text_sender,
            failure: None,
            failures: Vec::new(),
        }
    }

    pub fn fail(&mut self, error: impl Into<CompositionError>) {
        self.failure = self.failure.or(Some(error.into()));
    }

    pub(crate) fn record_failure(&mut self, component: NodeName) {
        if let Some(error) = self.failure.take() {
            self.nodes.truncate(1);
            self.nodes[0].clear_contents();
            self.shortcuts.clear();
            self.failures.push(ComponentFailure {
                component,
                cause: ComponentError::Composition(error),
            });
        }
    }

    pub(crate) fn handles(&self) -> ViewHandles {
        ViewHandles {
            memory: self.memory.clone(),
            motion: self.motion.clone(),
            text_sender: self.text_sender.clone(),
        }
    }

    pub fn text_sender(&self) -> TextSender {
        self.text_sender.clone()
    }

    pub fn viewport(&self) -> Viewport {
        self.local(&VIEWPORT)
    }

    pub fn motion(&self) -> &Motion {
        &self.motion
    }

    pub fn on_shortcut(&mut self, shortcut: Shortcut, action: impl FnMut() + 'static) {
        self.shortcuts.push(ComposedShortcut {
            node: self.nodes[self.parent].id,
            shortcut,
            action: Rc::new(RefCell::new(action)),
        });
    }

    pub fn provide<T: Clone + PartialEq + 'static, R>(
        &mut self,
        key: &'static Local<T>,
        value: T,
        children: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let saved = self.locals.open(key, value);
        let result = children(self);
        self.locals.close(saved);
        result
    }

    pub fn local<T: Clone + PartialEq + 'static>(&self, key: &'static Local<T>) -> T {
        self.locals.read(key)
    }

    pub(crate) fn resolve_text_style(&self, style: TextStyle) -> TextStyle<Dp> {
        style.resolve(&self.local(&TEXT_SCALING))
    }

    pub fn state<T: 'static>(&mut self, key: impl Hash, init: impl FnOnce() -> T) -> State<T> {
        let parent = self.nodes[self.parent].id;
        self.state_of(parent, key, init)
    }

    pub fn state_of<T: 'static>(
        &self,
        node: NodeId,
        key: impl Hash,
        init: impl FnOnce() -> T,
    ) -> State<T> {
        let key = (node, node::hash_of(key));
        self.memory.borrow_mut().state(key, init)
    }

    pub fn next_id(&self) -> NodeId {
        let parent = &self.nodes[self.parent];
        NodeId::at_position(parent.id, parent.children.len())
    }

    fn node(&mut self, id: NodeId, kind: NodeKind, modifier: Modifier) -> Node {
        let mut node = Node::new(id, kind, self.local(&INTERACTION_STYLE));
        if let Err(error) = modifier.apply(&mut node, self) {
            self.fail(error);
        }
        node
    }

    fn add(&mut self, node: Node, children: impl FnOnce(&mut Self)) {
        let index = self.nodes.len();
        self.nodes.push(node);
        self.nodes[self.parent].children.push(index);
        let outer = std::mem::replace(&mut self.parent, index);
        children(self);
        self.parent = outer;
    }

    fn leaf(&mut self, kind: NodeKind, modifier: Modifier) {
        let node = self.node(self.next_id(), kind, modifier);
        self.add(node, |_| {});
    }

    pub fn layout(
        &mut self,
        arrangement: impl Into<Arrangement>,
        modifier: Modifier,
        children: impl FnOnce(&mut Self),
    ) {
        let kind = NodeKind::Container(arrangement.into());
        let node = self.node(self.next_id(), kind, modifier);
        self.add(node, children);
    }

    pub fn key(&mut self, key: impl Hash, modifier: Modifier, children: impl FnOnce(&mut Self)) {
        let node = self.keyed(key, modifier);
        self.add(node, children);
    }

    fn keyed(&mut self, key: impl Hash, modifier: Modifier) -> Node {
        let id = NodeId::keyed(self.nodes[self.parent].id, key);
        let kind = NodeKind::Container(Arrangement::Overlay);
        self.node(id, kind, modifier)
    }

    pub fn view(
        &mut self,
        key: impl Hash,
        modifier: Modifier,
        children: impl FnOnce(&mut Self),
    ) -> ViewId {
        let node = self.keyed(key, modifier);
        let view = ViewId(node.id);
        self.add(node, |ui| ui.provide(&ENCLOSING_VIEW, view, children));
        view
    }

    pub fn crossing(&mut self, from: ViewId, to: ViewId, progress: &Animation<f64>) {
        let progress = progress.clone();
        let crossing = Crossing { from, to, progress };
        self.nodes[self.parent].crossings.push(crossing);
    }

    pub fn shared(&mut self, key: impl Hash, modifier: Modifier, children: impl FnOnce(&mut Self)) {
        let kind = NodeKind::Container(Arrangement::Overlay);
        let (view, key) = (self.local(&ENCLOSING_VIEW), SharedKey::of(key));
        let mut node = self.node(NodeId::shared(view, key), kind, modifier);
        node.identity = Identity::Shared { view, key };
        self.add(node, children);
    }

    pub fn text(&mut self, text: impl Into<String>, modifier: Modifier) {
        let style = self.resolve_text_style(self.local(&TEXT_STYLE));
        self.leaf(NodeKind::Text(text.into(), style), modifier);
    }

    pub fn icon(&mut self, icon: &'static Icon, options: IconOptions, modifier: Modifier) {
        let line = self.resolve_text_style(self.local(&TEXT_STYLE));
        let kind = NodeKind::Icon {
            icon,
            color: line.color,
            style: options.style,
            size: options.size.map_or(IconSize::Line(line), IconSize::Fixed),
        };
        self.leaf(kind, modifier);
    }

    pub fn canvas(
        &mut self,
        size: Size,
        modifier: Modifier,
        draw: impl Fn(&mut Drawing<'_>) + 'static,
    ) {
        let kind = NodeKind::Canvas {
            size,
            draw: Rc::new(draw),
            draw_context: DrawContext {
                viewport: self.viewport(),
                text_scaling: self.local(&TEXT_SCALING),
            },
        };
        self.leaf(kind, modifier);
    }

    pub fn spacer(&mut self, width: Dp, height: Dp, modifier: Modifier) {
        self.leaf(NodeKind::Spacer(Size::new(width, height)), modifier);
    }

    pub fn text_input(
        &mut self,
        value: impl Into<String>,
        options: TextFieldOptions,
        modifier: Modifier,
        change: impl FnMut(String) + 'static,
    ) {
        let content = FieldContent::Plain {
            value: value.into(),
            change: Rc::new(RefCell::new(change)),
            lines: Lines::One,
        };
        self.field(content, options, modifier);
    }

    pub fn protected_input(
        &mut self,
        value: crate::protected_text::ProtectedText,
        options: TextFieldOptions,
        modifier: Modifier,
    ) {
        let input = ProtectedInput::new(value, self.local(&PASSWORD_MASK));
        self.field(FieldContent::Protected(input), options, modifier);
    }

    pub fn text_area(
        &mut self,
        value: impl Into<String>,
        options: TextFieldOptions,
        modifier: Modifier,
        change: impl FnMut(String) + 'static,
    ) {
        let content = FieldContent::Plain {
            value: value.into(),
            change: Rc::new(RefCell::new(change)),
            lines: Lines::Several,
        };
        self.field(content, options, modifier);
    }

    fn field(&mut self, content: FieldContent, options: TextFieldOptions, modifier: Modifier) {
        let kind = NodeKind::TextInput {
            content,
            placeholder: options.placeholder,
            style: self.resolve_text_style(self.local(&TEXT_STYLE)),
            hints: options.hints,
        };
        self.leaf(kind, modifier);
    }
}
