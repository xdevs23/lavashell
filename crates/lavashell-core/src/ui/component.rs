// SPDX-License-Identifier: GPL-3.0-only
use super::{ComposedShortcut, Modifier, Node, Ui, ViewHandles};
use crate::local::{LocalReader, LocalScope};
use crate::reactive::{Phase, Scope};
use crate::view::NodeName;
use std::cell::Cell;
use std::cell::RefCell;
use std::hash::Hash;
use std::rc::Rc;

type Compose = Box<dyn FnMut(&mut Ui)>;

pub(crate) struct Component {
    pub scope: Scope,
    compose: RefCell<Compose>,
    locals: LocalScope,
    pub shortcuts: RefCell<Vec<ComposedShortcut>>,
    composed: Cell<usize>,
}

impl Component {
    /// Composes the component of `node`. The nodes and shortcuts of a failed composition are dropped, and its failure
    /// is the last failure of the returned composition. In either case, the component composes again once a value read
    /// by its composition changes.
    pub(crate) fn build(&self, node: &Node, handles: ViewHandles) -> Ui {
        self.scope.track(Phase::Composition, || {
            let locals = LocalReader::new(self.locals.clone());
            let mut ui = Ui::composing(handles, locals);
            ui.nodes.reserve(self.composed.get());
            ui.nodes[0].id = node.id;
            (self.compose.borrow_mut())(&mut ui);
            ui.record_failure(NodeName::of(node));
            self.composed.set(ui.nodes.len());
            *self.shortcuts.borrow_mut() = std::mem::take(&mut ui.shortcuts);
            ui
        })
    }

    /// Whether the layout and the paint of the component follow from its nodes alone: neither read a value, and
    /// neither is dirty.
    pub(crate) fn lays_out_and_paints_from_its_nodes(&self) -> bool {
        [Phase::Layout, Phase::Paint]
            .into_iter()
            .all(|phase| !self.scope.has_read(phase) && !self.scope.dirty().contains(phase))
    }
}

impl Ui {
    /// Adds an overlay node identified by `key` and composes its children with `compose`. When a signal read inside
    /// `compose` changes, the view runs `compose` again without composing the parent again. `compose` must capture
    /// signal handles: a signal value read by the parent before this call subscribes the parent instead. Locals read
    /// inside `compose` have the values set around this call.
    pub fn scoped(
        &mut self,
        key: impl Hash,
        modifier: Modifier,
        compose: impl FnMut(&mut Ui) + 'static,
    ) {
        let mut node = self.keyed(key, modifier);
        let component = Rc::new(Component {
            scope: Scope::default(),
            compose: RefCell::new(Box::new(compose)),
            locals: self.locals.scope().clone(),
            shortcuts: RefCell::default(),
            composed: Cell::new(0),
        });
        let composed = component.build(&node, self.handles());
        node.component = Some(component);
        self.add(node, |ui| ui.attach(composed));
    }

    /// Moves the nodes and shortcuts composed in `ui` into this `Ui`. The root node of `ui` becomes the contents of
    /// the node to which nodes added here go.
    fn attach(&mut self, mut ui: Ui) {
        let offset = self.nodes.len() - 1;
        self.nodes[self.parent].take_contents(&mut ui.nodes[0]);
        for child in &mut self.nodes[self.parent].children {
            *child += offset;
        }
        for mut node in ui.nodes.drain(1..) {
            for child in &mut node.children {
                *child += offset;
            }
            self.nodes.push(node);
        }
        self.shortcuts.extend(ui.shortcuts);
        self.failures.extend(ui.failures);
    }
}
