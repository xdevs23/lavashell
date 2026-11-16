// SPDX-License-Identifier: GPL-3.0-only
use super::View;
use super::parents::ROOT;
use crate::reactive::Phase;
use crate::ui::{Node, NodeId};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Refresh {
    Unchanged,
    Recomposed,
}

impl View {
    pub(super) fn refresh_components(&mut self) -> Refresh {
        let mut replacements = BTreeMap::new();
        let mut covered = BTreeSet::new();
        let mut stale = BTreeSet::new();
        let mut failures = Vec::new();
        for (index, node) in self.nodes.iter().enumerate() {
            if covered.contains(&index) {
                continue;
            }
            let Some(component) = &node.component else {
                continue;
            };
            if !component.scope.dirty().contains(Phase::Composition)
                || self.failures.failed_in_frame(node.id)
            {
                continue;
            }
            let mut ui = component.build(node, self.handles());
            failures.append(&mut ui.failures);
            let mut root = node.clone();
            root.take_contents(&mut ui.nodes[0]);
            ui.nodes[0] = root;
            let _no_other_replacement_at_the_index = replacements.insert(index, ui.nodes);
            self.stats.component_compositions += 1;
            let _always_new_as_ids_are_unique = stale.insert(node.id);
            collect_descendants(&self.nodes, index, &mut covered);
        }
        self.failures.extend(failures);
        if replacements.is_empty() {
            return Refresh::Unchanged;
        }
        self.failures.drop_layout_retries(&stale);
        stale.extend(self.ids_of_changed_nodes(&covered, &replacements));
        stale.extend(self.ids_of_components_reading_values(&covered));
        self.retained.remove(&stale);
        let mut nodes = Vec::with_capacity(self.nodes.len());
        append(&self.nodes, ROOT, &mut replacements, &mut nodes);
        self.replace_tree(nodes);
        Refresh::Recomposed
    }

    fn ids_of_changed_nodes(
        &self,
        covered: &BTreeSet<usize>,
        replacements: &BTreeMap<usize, Vec<Node>>,
    ) -> Vec<NodeId> {
        let rebuilt: BTreeMap<_, _> = (replacements.values())
            .flat_map(|tree| {
                tree.iter()
                    .map(move |node| (node.id, (node, tree.as_slice())))
            })
            .collect();
        (covered.iter().map(|index| &self.nodes[*index]))
            .filter(|&earlier| {
                !rebuilt
                    .get(&earlier.id)
                    .is_some_and(|(node, tree)| node.is_built_like(tree, earlier, &self.nodes))
            })
            .map(|earlier| earlier.id)
            .collect()
    }

    fn ids_of_components_reading_values(&self, covered: &BTreeSet<usize>) -> Vec<NodeId> {
        (covered.iter().map(|index| &self.nodes[*index]))
            .filter(|earlier| {
                (earlier.component.as_ref())
                    .is_some_and(|component| !component.lays_out_and_paints_from_its_nodes())
            })
            .map(|earlier| earlier.id)
            .collect()
    }
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

pub(super) fn append(
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
