// SPDX-License-Identifier: GPL-3.0-only
use super::View;
use super::component::append;
use super::frame_error::{ComponentError, ComponentFailure, LayoutError, NodeName};
use super::frame_result::Failures;
use super::graph::Graph;
use super::parents::{Parents, ROOT};
use crate::animation::AnimationError;
use crate::reactive::{Phase, Scope};
use crate::ui::{Node, NodeId};
use crate::view::component::{collect_descendants, index_of};
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct Failed {
    pub inside: usize,
    pub cause: ComponentError,
}

#[derive(Default)]
pub(super) struct FrameFailures {
    carried: Vec<ComponentFailure>,
    recorded: Vec<ComponentFailure>,
    pub paint: Vec<Failed>,
    pub layout_retries: Vec<LayoutRetry>,
}

impl FrameFailures {
    pub fn record(&mut self, failure: ComponentFailure) {
        self.recorded.push(failure);
    }

    pub fn failed_in_frame(&self, id: NodeId) -> bool {
        (self.recorded.iter()).any(|failure| failure.component.id == id)
    }

    fn cycled_in_frame(&self) -> impl Iterator<Item = NodeId> + '_ {
        (self.recorded.iter())
            .filter(|failure| matches!(failure.cause, ComponentError::CompositionCycle))
            .map(|failure| failure.component.id)
    }

    pub fn start_frame(&mut self) {
        let recorded = std::mem::take(&mut self.recorded);
        self.carried.extend(recorded);
    }

    pub fn take(&mut self) -> Failures {
        let mut failures = std::mem::take(&mut self.carried);
        failures.append(&mut self.recorded);
        Failures(failures)
    }
}

impl Extend<ComponentFailure> for FrameFailures {
    fn extend<I: IntoIterator<Item = ComponentFailure>>(&mut self, failures: I) {
        self.recorded.extend(failures);
    }
}

pub(super) struct LayoutRetry {
    composition: NodeId,
    dropped: Vec<Scope>,
}

impl LayoutRetry {
    pub fn is_due(&self) -> bool {
        (self.dropped.iter()).any(|scope| scope.dirty().contains(Phase::Composition))
    }
}

pub(super) struct Repaired {
    pub nodes: Vec<Node>,
    pub parents: Parents,
    pub graph: Graph,
    pub dropped: BTreeSet<NodeId>,
}

struct Emptied {
    nodes: Vec<Node>,
    compositions: BTreeSet<NodeId>,
    dropped: BTreeSet<NodeId>,
    scopes: Vec<Scope>,
}

impl View {
    pub(super) fn fail(&mut self, failed: impl IntoIterator<Item = Failed>) -> bool {
        let mut emptied = self.record(failed);
        let lost_nodes = !emptied.is_empty();
        while !emptied.is_empty() {
            let tree = without_contents(&self.nodes, &emptied);
            self.forget_contents(&tree);
            let repaired = self.repaired(tree.nodes, tree.dropped);
            let failed_exits = self.reconcile(repaired);
            emptied = self.record(failed_exits);
        }
        self.redraw |= lost_nodes;
        lost_nodes
    }

    pub(super) fn fail_cycle(&mut self, index: usize) {
        let cycle = Failed {
            inside: index,
            cause: ComponentError::CompositionCycle,
        };
        let _lost_nodes = self.fail([cycle]);
    }

    pub(super) fn fail_paint_failures(&mut self) -> bool {
        let failed = std::mem::take(&mut self.failures.paint);
        self.fail(failed)
    }

    pub(super) fn repaired(
        &mut self,
        mut nodes: Vec<Node>,
        mut dropped: BTreeSet<NodeId>,
    ) -> Repaired {
        let earlier = std::mem::take(&mut self.failures.layout_retries);
        let (parents, graph) = loop {
            let parents = Parents::new(&nodes);
            match check(&nodes, &parents) {
                Ok(graph) => break (parents, graph),
                Err(error) => {
                    let composition = composition_around_named(&nodes, &parents, &error);
                    self.failures.record(ComponentFailure {
                        component: NodeName::of(&nodes[composition]),
                        cause: ComponentError::Layout(error),
                    });
                    let tree = without_contents(&nodes, &BTreeSet::from([composition]));
                    self.forget_contents(&tree);
                    self.failures.layout_retries.push(LayoutRetry {
                        composition: nodes[composition].id,
                        dropped: tree.scopes,
                    });
                    dropped.extend(tree.dropped);
                    nodes = tree.nodes;
                }
            }
        };
        for retry in earlier {
            if let Some(scope) = self.composition_scope(&nodes, retry.composition) {
                scope.invalidate(Phase::Composition);
            }
        }
        Repaired {
            nodes,
            parents,
            graph,
            dropped,
        }
    }

    pub(super) fn retry_changed_layouts(&mut self) {
        let (due, waiting): (Vec<_>, Vec<_>) = (std::mem::take(&mut self.failures.layout_retries)
            .into_iter())
        .partition(LayoutRetry::is_due);
        self.failures.layout_retries = waiting;
        for retry in due {
            if let Some(scope) = self.composition_scope(&self.nodes, retry.composition) {
                scope.invalidate(Phase::Composition);
            }
        }
    }

    pub(super) fn settle_cycles(&self) {
        for id in self.failures.cycled_in_frame() {
            if let Some(scope) = self.composition_scope(&self.nodes, id) {
                scope.clean(Phase::Composition);
            }
        }
    }

    fn record(&mut self, failed: impl IntoIterator<Item = Failed>) -> BTreeSet<usize> {
        let mut emptied = BTreeSet::new();
        for Failed { inside, cause } in failed {
            let composition = composition_at_or_above(&self.nodes, &self.parents, inside);
            let node = &self.nodes[composition];
            if !node.children.is_empty() {
                let _failed_before_in_this_round = emptied.insert(composition);
            }
            self.failures.record(ComponentFailure {
                component: NodeName::of(node),
                cause,
            });
        }
        emptied
    }

    fn forget_contents(&mut self, tree: &Emptied) {
        for node in (tree.nodes.iter()).filter(|node| tree.compositions.contains(&node.id)) {
            match &node.component {
                Some(component) => component.shortcuts.borrow_mut().clear(),
                None => self.routing.root_shortcuts.clear(),
            }
        }
        self.retained.remove(&tree.compositions);
    }

    fn composition_scope(&self, nodes: &[Node], id: NodeId) -> Option<Scope> {
        let node = &nodes[index_of(nodes, id)?];
        Some(match &node.component {
            Some(component) => component.scope.clone(),
            None => self.scope.clone(),
        })
    }
}

impl FrameFailures {
    pub(super) fn fail_paint(&mut self, composed_parent: usize, error: AnimationError) {
        self.paint.push(Failed {
            inside: composed_parent,
            cause: ComponentError::Paint(error),
        });
    }
    pub(super) fn drop_layout_retries(&mut self, composed: &BTreeSet<NodeId>) {
        (self.layout_retries).retain(|retry| !composed.contains(&retry.composition));
    }
}

fn without_contents(nodes: &[Node], emptied: &BTreeSet<usize>) -> Emptied {
    let mut replacements = BTreeMap::new();
    let mut compositions = BTreeSet::new();
    let mut removed = BTreeSet::new();
    for &index in emptied {
        let mut node = nodes[index].clone();
        node.clear_contents();
        let _emptied_once = compositions.insert(node.id);
        let _no_other_replacement_at_the_index = replacements.insert(index, vec![node]);
        for &child in &nodes[index].children {
            collect_descendants(nodes, child, &mut removed);
        }
    }
    let mut kept = Vec::with_capacity(nodes.len());
    append(nodes, ROOT, &mut replacements, &mut kept);
    Emptied {
        nodes: kept,
        compositions,
        dropped: removed.iter().map(|&index| nodes[index].id).collect(),
        scopes: (removed.iter())
            .filter_map(|&index| Some(nodes[index].component.as_ref()?.scope.clone()))
            .collect(),
    }
}

fn check(nodes: &[Node], parents: &Parents) -> Result<Graph, LayoutError> {
    let mut present = BTreeSet::new();
    if let Some(repeated) = nodes.iter().find(|node| !present.insert(node.id)) {
        return Err(LayoutError::DuplicateKey(NodeName::of(repeated)));
    }
    Graph::new(nodes, parents)
}

fn composition_at_or_above(nodes: &[Node], parents: &Parents, index: usize) -> usize {
    (parents.ancestry(index))
        .find(|&node| nodes[node].component.is_some())
        .unwrap_or(ROOT)
}

fn composition_around_named(nodes: &[Node], parents: &Parents, error: &LayoutError) -> usize {
    let named = error.nodes();
    let around = (nodes.iter().enumerate())
        .filter(|(_, node)| named.contains(&node.id))
        .map(|(index, _)| parents.around(index))
        .reduce(|first, second| common_ancestor(parents, first, second));
    composition_at_or_above(nodes, parents, around.unwrap_or(ROOT))
}

fn common_ancestor(parents: &Parents, first: usize, second: usize) -> usize {
    let above_first: BTreeSet<_> = parents.ancestry(first).collect();
    (parents.ancestry(second))
        .find(|node| above_first.contains(node))
        .unwrap_or(ROOT)
}
