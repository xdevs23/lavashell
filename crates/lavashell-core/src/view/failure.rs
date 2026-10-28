// SPDX-License-Identifier: GPL-3.0-only
use super::component::append;
use super::frame_error::{ComponentError, ComponentFailure, LayoutError, NodeName};
use super::frame_result::Failures;
use super::graph::Graph;
use super::parents::{Parents, ROOT};
use super::{BTreeMap, BTreeSet, Node, NodeId, Phase, View, collect_descendants, index_of};
use crate::animation::AnimationError;
use crate::reactive::Scope;

/// A failure in the composition at or above the node at the index `inside`: the nearest component at or above that
/// node, or the root composition.
pub(super) struct Failed {
    pub inside: usize,
    pub cause: ComponentError,
}

/// The component failures that the next frame of a view returns.
#[derive(Default)]
pub(super) struct FrameFailures {
    /// The failures from before the frame being made, such as those of a measurement or of a frame whose update was
    /// not applied.
    carried: Vec<ComponentFailure>,
    /// The failures recorded in the frame being made.
    recorded: Vec<ComponentFailure>,
}

impl FrameFailures {
    pub fn record(&mut self, failure: ComponentFailure) {
        self.recorded.push(failure);
    }

    /// Whether the composition of the node `id` failed in the frame being made. Such a composition does not compose
    /// again in that frame.
    pub fn failed_in_frame(&self, id: NodeId) -> bool {
        (self.recorded.iter()).any(|failure| failure.component.id == id)
    }

    /// The ids of the nodes whose compositions failed in the frame being made because each layout made them compose
    /// again.
    fn cycled_in_frame(&self) -> impl Iterator<Item = NodeId> + '_ {
        (self.recorded.iter())
            .filter(|failure| matches!(failure.cause, ComponentError::CompositionCycle))
            .map(|failure| failure.component.id)
    }

    /// Starts a frame. The frame returns the failures recorded before it, which do not stop their compositions from
    /// composing in it.
    pub fn start_frame(&mut self) {
        let recorded = std::mem::take(&mut self.recorded);
        self.carried.extend(recorded);
    }

    /// Takes every failure that the frame being made returns, in the order of the failures.
    pub fn take(&mut self) -> Failures {
        let mut failures = std::mem::take(&mut self.carried);
        failures.append(&mut self.recorded);
        Failures(failures)
    }

    /// Puts back the failures of a frame whose update was not applied, so that the next frame returns them.
    pub fn carry(&mut self, failures: Failures) {
        self.carried.extend(failures);
    }
}

impl Extend<ComponentFailure> for FrameFailures {
    fn extend<I: IntoIterator<Item = ComponentFailure>>(&mut self, failures: I) {
        self.recorded.extend(failures);
    }
}

/// A composition emptied because the tree could not be laid out. The cause can lie outside the composition, so the
/// composition composes again once the tree changes, or once a value read by a component dropped with its contents
/// changes.
pub(super) struct LayoutRetry {
    composition: NodeId,
    /// The scopes of the components dropped with the contents.
    dropped: Vec<Scope>,
}

impl LayoutRetry {
    /// Whether a value read by a component dropped with the contents changed.
    pub fn is_due(&self) -> bool {
        (self.dropped.iter()).any(|scope| scope.dirty().contains(Phase::Composition))
    }
}

/// A composed tree that can be laid out, with the parents of its nodes and the reads between them. `dropped` contains
/// the ids of the nodes removed with the contents of failed compositions, which leave without an exit.
pub(super) struct Repaired {
    pub nodes: Vec<Node>,
    pub parents: Parents,
    pub graph: Graph,
    pub dropped: BTreeSet<NodeId>,
}

/// A tree with the contents of failed compositions removed.
struct Emptied {
    nodes: Vec<Node>,
    /// The ids of the emptied compositions.
    compositions: BTreeSet<NodeId>,
    /// The ids of the removed nodes.
    dropped: BTreeSet<NodeId>,
    /// The scopes of the removed components.
    scopes: Vec<Scope>,
}

impl View {
    /// Records each failure in `failed` for the composition at or above the node at its index, and empties those
    /// compositions. Returns whether a composition lost nodes, so that the tree needs a new layout.
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

    /// Fails the composition of the node at `index`, which composes again after each layout.
    pub(super) fn fail_cycle(&mut self, index: usize) {
        let cycle = Failed {
            inside: index,
            cause: ComponentError::CompositionCycle,
        };
        let _lost_nodes = self.fail([cycle]);
    }

    /// Records `error`, raised by an animation of a node painted among the children of the composed node at
    /// `composed_parent`. The composition at or above that node fails once the paint ends.
    pub(super) fn fail_paint(&mut self, composed_parent: usize, error: AnimationError) {
        self.paint_failures.push(Failed {
            inside: composed_parent,
            cause: ComponentError::Paint(error),
        });
    }

    /// Fails the composition around each node whose animation failed in the last paint. Returns whether a composition
    /// lost nodes, so that the tree needs a new layout and a new paint.
    pub(super) fn fail_paint_failures(&mut self) -> bool {
        let failed = std::mem::take(&mut self.paint_failures);
        self.fail(failed)
    }

    /// `nodes` emptied until the tree can be laid out. Each round records the layout error of the tree as the failure
    /// of the nearest composition around the nodes named by the error, and empties that composition. Each composition
    /// emptied that way before composes again, as the tree changed. `dropped` contains the ids of the nodes already
    /// removed with the contents of failed compositions.
    pub(super) fn repaired(
        &mut self,
        mut nodes: Vec<Node>,
        mut dropped: BTreeSet<NodeId>,
    ) -> Repaired {
        let earlier = std::mem::take(&mut self.layout_retries);
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
                    self.layout_retries.push(LayoutRetry {
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

    /// Drops the retries of the compositions `composed` after a layout error, as those compositions compose again.
    pub(super) fn drop_layout_retries(&mut self, composed: &BTreeSet<NodeId>) {
        (self.layout_retries).retain(|retry| !composed.contains(&retry.composition));
    }

    /// Makes each composition emptied by a layout error compose again once a value read by a component dropped with
    /// its contents changed.
    pub(super) fn retry_changed_layouts(&mut self) {
        let (due, waiting): (Vec<_>, Vec<_>) =
            (std::mem::take(&mut self.layout_retries).into_iter()).partition(LayoutRetry::is_due);
        self.layout_retries = waiting;
        for retry in due {
            if let Some(scope) = self.composition_scope(&self.nodes, retry.composition) {
                scope.invalidate(Phase::Composition);
            }
        }
    }

    /// Clears the dirty flag of each composition that failed in this frame because each layout made it compose again.
    /// Only a change from outside the layouts of the frame then makes it compose again.
    pub(super) fn settle_cycles(&self) {
        for id in self.failures.cycled_in_frame() {
            if let Some(scope) = self.composition_scope(&self.nodes, id) {
                scope.clean(Phase::Composition);
            }
        }
    }

    /// Records each failure in `failed` for the composition at or above the node at its index. Returns the indices of
    /// those compositions that have children.
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

    /// Drops the shortcuts of the compositions emptied in `tree`, and the retained layouts and paintings that contain
    /// their contents.
    fn forget_contents(&mut self, tree: &Emptied) {
        for node in (tree.nodes.iter()).filter(|node| tree.compositions.contains(&node.id)) {
            match &node.component {
                Some(component) => component.shortcuts.borrow_mut().clear(),
                None => self.root_shortcuts.clear(),
            }
        }
        self.retained.remove(&tree.compositions);
    }

    /// The scope of the composition of the node `id` in `nodes`: the scope of its component, or the scope of the root
    /// composition for the root node.
    fn composition_scope(&self, nodes: &[Node], id: NodeId) -> Option<Scope> {
        let node = &nodes[index_of(nodes, id)?];
        Some(match &node.component {
            Some(component) => component.scope.clone(),
            None => self.scope.clone(),
        })
    }
}

/// `nodes` with the children and the exit actions removed from each node whose index is in `emptied`.
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

/// Checks that `nodes` have unique ids and can be laid out, and returns the reads between them.
fn check(nodes: &[Node], parents: &Parents) -> Result<Graph, LayoutError> {
    let mut present = BTreeSet::new();
    if let Some(repeated) = nodes.iter().find(|node| !present.insert(node.id)) {
        return Err(LayoutError::DuplicateKey(NodeName::of(repeated)));
    }
    Graph::new(nodes, parents)
}

/// The index of the composition that composed the children of the node at `index`: the nearest component at or above
/// that node, or the root node for the root composition.
fn composition_at_or_above(nodes: &[Node], parents: &Parents, index: usize) -> usize {
    (parents.ancestry(index))
        .find(|&node| nodes[node].component.is_some())
        .unwrap_or(ROOT)
}

/// The index of the composition that composed every node named by `error`: the nearest component around all of them,
/// or the root node for the root composition.
fn composition_around_named(nodes: &[Node], parents: &Parents, error: &LayoutError) -> usize {
    let named = error.nodes();
    let around = (nodes.iter().enumerate())
        .filter(|(_, node)| named.contains(&node.id))
        .map(|(index, _)| parents.around(index))
        .reduce(|first, second| common_ancestor(parents, first, second));
    composition_at_or_above(nodes, parents, around.unwrap_or(ROOT))
}

/// The nearest node at or above both the node at `first` and the node at `second`.
fn common_ancestor(parents: &Parents, first: usize, second: usize) -> usize {
    let above_first: BTreeSet<_> = parents.ancestry(first).collect();
    (parents.ancestry(second))
        .find(|node| above_first.contains(node))
        .unwrap_or(ROOT)
}
