// SPDX-License-Identifier: GPL-3.0-only
use super::frame_error::LayoutError;
use super::{BTreeMap, BTreeSet, Node, NodeId, Phase, View};
use std::rc::Rc;

/// Whether `refresh_components` composed any component again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Refresh {
    Unchanged,
    Recomposed,
}

impl View {
    /// Composes again the components whose composition is dirty, and puts the nodes they build in place of their old
    /// nodes.
    pub(super) fn refresh_components(&mut self) -> Result<Refresh, LayoutError> {
        let mut replacements = BTreeMap::new();
        // `nodes` lists each ancestor before its descendants. Composing an ancestor again builds every component
        // inside it again. `covered` contains those descendants, which the loop skips.
        let mut covered = BTreeSet::new();
        let mut stale = BTreeSet::new();
        for (index, node) in self.nodes.iter().enumerate() {
            if covered.contains(&index) {
                continue;
            }
            let Some(component) = &node.component else {
                continue;
            };
            if !component.scope.dirty().contains(Phase::Composition) {
                continue;
            }
            let mut ui = component.build(node.id, self.handles());
            if let Some(error) = ui.failure {
                return Err(LayoutError::Composition(error));
            }
            let mut root = node.clone();
            root.take_contents(&mut ui.nodes[0]);
            ui.nodes[0] = root;
            let _no_other_replacement_at_the_index = replacements.insert(index, ui.nodes);
            self.stats.component_compositions += 1;
            let _always_new_as_ids_are_unique = stale.insert(node.id);
            collect_descendants(&self.nodes, index, &mut covered);
        }
        if replacements.is_empty() {
            return Ok(Refresh::Unchanged);
        }
        let mut nodes = Vec::with_capacity(self.nodes.len());
        append(&self.nodes, 0, &mut replacements, &mut nodes);
        stale.extend(changed_readers(&nodes, &self.nodes));
        self.retained.remove(&stale);
        self.reconcile(nodes)?;
        Ok(Refresh::Recomposed)
    }
}

/// The ids of the components in `nodes` that were created again inside a component composed again, and that read
/// other values than the earlier instance with the same id in `old`. The retained paint and layout of a component
/// created again are valid only while the new instance reads the same values as the earlier one.
fn changed_readers(nodes: &[Node], old: &[Node]) -> Vec<NodeId> {
    let previous: BTreeMap<_, _> = old
        .iter()
        .filter_map(|node| Some((node.id, node.component.as_ref()?)))
        .collect();
    nodes
        .iter()
        .filter(|node| {
            let (Some(component), Some(before)) = (&node.component, previous.get(&node.id)) else {
                return false;
            };
            !Rc::ptr_eq(component, before) && *component.reads.borrow() != *before.reads.borrow()
        })
        .map(|node| node.id)
        .collect()
}

pub(super) fn index_of(nodes: &[Node], id: NodeId) -> Option<usize> {
    nodes.iter().position(|node| node.id == id)
}

pub(super) fn collect_descendants(nodes: &[Node], index: usize, result: &mut BTreeSet<usize>) {
    let _already_covered_by_an_outer_component = result.insert(index);
    for child in &nodes[index].children {
        collect_descendants(nodes, *child, result);
    }
}

fn append(
    source: &[Node],
    index: usize,
    replacements: &mut BTreeMap<usize, Vec<Node>>,
    target: &mut Vec<Node>,
) {
    if let Some(replacement) = replacements.remove(&index) {
        append(&replacement, 0, &mut BTreeMap::new(), target);
        return;
    }
    let result = target.len();
    let mut node = source[index].clone();
    node.children.clear();
    target.push(node);
    for child in &source[index].children {
        let appended = target.len();
        append(source, *child, replacements, target);
        target[result].children.push(appended);
    }
}
