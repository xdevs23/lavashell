// SPDX-License-Identifier: GPL-3.0-only
mod accessibility;
mod animate;
mod component;
mod deferred;
mod modifier;
mod node;
mod pointer;
mod scrolling;
mod settings;

use component::Component;
pub(crate) use deferred::laid_out::{Measured, Placed};
pub use deferred::{Bounds, Deferred, NodeRef, Reach, Sizes};
pub(crate) use deferred::{NodeRefKey, Part, Read, Used};
pub use modifier::{AnimatedColor, Modifier, ModifierElement, ModifierValue};
pub(crate) use modifier::{LayoutValue, MainLength};
pub(crate) use node::Memory;
pub use node::{
    AccessibilityHandler, Action, ActivateAction, Activation, Align, Arrangement, ContentSizes,
    ContentSizesAction, FieldContent, Flow, IconOptions, IconSize, IconStyle, Identity,
    InputModifiers, KeyAction, Lines, MeasureAction, Node, NodeId, NodeKind, ScrollAction,
    SemanticsProvider, TextChange, TextFieldOptions, Tooltip, Weight,
};
pub use pointer::{
    DragHandler, DragInput, DragPhase, Draggable, PointerAction, PointerId, PointerInput,
    PointerKind, PointerPhase, TouchId, TransformHandler, TransformInput,
};
pub(crate) use scrolling::Scroller;

use crate::animation::Motion;
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
use crate::view::{ComponentError, ComponentFailure, CompositionError, NodeName};
use std::cell::RefCell;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

/// The parts of a view shared by every composition of that view: the remembered state, the motion
/// clock and the queue of edits for the focused field.
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

/// A shortcut with its action, and the node in which it was composed. The shortcut works while that node is shown.
#[derive(Clone)]
pub(crate) struct ComposedShortcut {
    pub node: NodeId,
    pub shortcut: Shortcut,
    pub action: Action,
}

pub struct Ui {
    pub(crate) nodes: Vec<Node>,
    /// The index in `nodes` of the current parent node, to which the next added nodes go as children.
    parent: usize,
    pub(crate) memory: Memory,
    locals: LocalReader,
    pub(crate) shortcuts: Vec<ComposedShortcut>,
    pub(crate) motion: Motion,
    pub(crate) text_sender: TextSender,
    /// The first error passed to `fail`. The composition fails with this error.
    failure: Option<CompositionError>,
    /// The failures of the components composed inside this composition, followed by the failure of this
    /// composition.
    pub(crate) failures: Vec<ComponentFailure>,
}

impl Ui {
    /// A composition with one root overlay node that uses the shared parts `handles` of its view and
    /// reads locals through `locals`.
    pub(crate) fn composing(handles: ViewHandles, locals: LocalReader) -> Self {
        let ViewHandles {
            memory,
            motion,
            text_sender,
        } = handles;
        Self {
            nodes: vec![Node::new(
                NodeId(1),
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

    /// Fails the component that composes here with `error`, for a component that cannot compose. The
    /// component then contributes no node to the tree, and the next frame returns the first error of
    /// its composition. The parent and the other components are unchanged.
    pub fn fail(&mut self, error: impl Into<CompositionError>) {
        self.failure = self.failure.or(Some(error.into()));
    }

    /// Drops every node and shortcut built by this composition when it failed, and records the first
    /// error of the composition as the failure of `component`. The failures of the components
    /// composed inside remain in the composition.
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

    /// Composes `children` with `value` provided for the local `key`. Inside `children`, scoped
    /// components included, reading `key` returns `value`, unless a nearer call to `provide` provides
    /// another value. A nested component composes again when `value` does not compare equal to the
    /// value it read before. Build a value compared by identity, such as `StateStyles`, once and clone
    /// it, so that it compares equal across compositions.
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

    /// The value of the local `key` from the nearest enclosing `provide`, else the root value set on
    /// the view, else the default of `key`. The scoped component, or the root composition, in which
    /// this read happens composes again when the value changes.
    pub fn local<T: Clone + PartialEq + 'static>(&self, key: &'static Local<T>) -> T {
        self.locals.read(key)
    }

    pub(crate) fn resolve_text_style(&self, style: TextStyle) -> TextStyle<Dp> {
        style.resolve(&self.local(&TEXT_SCALING))
    }

    /// The state remembered for `key` on the current parent node, to which nodes added here go.
    pub fn state<T: 'static>(&mut self, key: impl Hash, init: impl FnOnce() -> T) -> State<T> {
        let parent = self.nodes[self.parent].id;
        self.state_of(parent, key, init)
    }

    /// The state remembered for `key` on `node`. Inside that node, `Ui::state` returns the same state
    /// for `key`. Handlers set on the node through its modifier can read and change this state.
    pub fn state_of<T: 'static>(
        &self,
        node: NodeId,
        key: impl Hash,
        init: impl FnOnce() -> T,
    ) -> State<T> {
        let mut hash = DefaultHasher::new();
        key.hash(&mut hash);
        let key = (node, hash.finish());
        self.memory.borrow_mut().state(key, init)
    }

    /// The ID of the node added by the next call on this `Ui`, unless that call gives the node a key.
    pub fn next_id(&self) -> NodeId {
        let parent = &self.nodes[self.parent];
        let mut hash = DefaultHasher::new();
        parent.id.hash(&mut hash);
        parent.children.len().hash(&mut hash);
        NodeId(hash.finish())
    }

    /// A node of `kind` with `id`, configured by `modifier` with the interaction style in effect here.
    /// A modifier that reads an animation of another view fails the component that composes here.
    fn node(&mut self, id: NodeId, kind: NodeKind, modifier: Modifier) -> Node {
        let mut node = Node::new(id, kind, self.local(&INTERACTION_STYLE));
        if let Err(error) = modifier.apply(&mut node, self) {
            self.fail(error);
        }
        node
    }

    /// Adds `node` as a child of the current parent node and composes `children` inside `node`.
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

    /// A container node that lays out `children` with `arrangement`.
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

    /// An overlay node whose ID comes from `key` and the ID of the current parent node, so `key`
    /// needs to be unique only among the children of that parent.
    fn keyed(&mut self, key: impl Hash, modifier: Modifier) -> Node {
        let mut hash = DefaultHasher::new();
        self.nodes[self.parent].id.hash(&mut hash);
        "key".hash(&mut hash);
        key.hash(&mut hash);
        let kind = NodeKind::Container(Arrangement::Overlay);
        self.node(NodeId(hash.finish()), kind, modifier)
    }

    /// An overlay node whose ID comes from `key` alone. The node can move between parents without
    /// losing its transition geometry or the state remembered for its children. `key` must be unique
    /// within the view.
    pub fn shared(&mut self, key: impl Hash, modifier: Modifier, children: impl FnOnce(&mut Self)) {
        let mut hash = DefaultHasher::new();
        "shared".hash(&mut hash);
        key.hash(&mut hash);
        let kind = NodeKind::Container(Arrangement::Overlay);
        let mut node = self.node(NodeId(hash.finish()), kind, modifier);
        node.identity = Identity::InView;
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

    /// A single-line field for secret text, such as a password. The value never appears in the
    /// strings of the scene or in accessibility. When the value is revealed, the field draws it with
    /// a `DrawCommand::PrivateText`, whose debug output is redacted. Copy, cut, drag and clipboard
    /// paste are disabled for this field. While the value is hidden, the field shows the
    /// `PASSWORD_MASK` set by the platform in place of each character.
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

    /// A field that edits `content`, in the text style composed here.
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
