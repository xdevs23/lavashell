// SPDX-License-Identifier: GPL-3.0-only
use super::frame_error::{LayoutError, LayoutRead, LayoutReads, NodeName, NodeValue};
use super::layout::{fill_decided_by, fills_through_children, weight};
use super::parents::{Parent, Parents};
use crate::geometry::Axis;
use crate::ui::{ModifierElement, Node, NodeRef, NodeRefKey, Part, Read, Used};
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};

/// The reads between nodes made by layout values, such as a width that reads the size of another node. The view
/// builds this graph from each newly composed tree, and the layout uses it to order the measuring of children.
#[derive(Default)]
pub(super) struct Graph {
    references: BTreeMap<NodeRefKey, usize>,
    /// For each container whose children read the sizes of each other, the positions of its children in the order in
    /// which the container measures them.
    orders: BTreeMap<usize, Vec<usize>>,
    sizes: Vec<Edge>,
    /// The nodes on the way from a node that reads the size of another node up to the container of
    /// both, and from that container down to the other node.
    crossed: BTreeSet<usize>,
    /// For each node whose natural width a size or a condition reads, the reads that the probe of that width makes
    /// from inside the node to the nodes outside it.
    probes: BTreeMap<usize, Vec<Edge>>,
    /// The nodes whose natural width is read by an offset or a position after measuring.
    nodes_with_natural_width_read_by_placement: BTreeSet<usize>,
}

/// One read during layout: the value that the node `reader` uses as `used` reads the `part` of the node `source`.
#[derive(Clone, Copy, PartialEq)]
struct Edge {
    reader: usize,
    used: Used,
    source: usize,
    part: Part,
}

/// A read of a size, moved up to the nearest container of both nodes. The container measures its child at the
/// position `from`, which contains the source, before its child at the position `to`, which contains the reader.
#[derive(Clone, Copy)]
struct Lifted {
    from: usize,
    to: usize,
    edge: Edge,
}

impl Graph {
    /// Builds the graph for the composed `nodes`. Fails on a weight that is not a finite number above zero, on a
    /// value of a modifier that is not valid, on a reference set on two nodes, and on a read of a reference set on no
    /// node.
    ///
    /// A read of a natural width counts as the reads that the probe of that width makes from inside its node to the
    /// nodes outside it. Fails on a probe that reads the natural width that it measures. Fails on a size or a
    /// condition that reads the size of the node carrying it, of a node around that node or of a node inside it.
    /// Fails on a read of the size of a child with a weight by a child without a weight, by the condition of a child
    /// with a weight or by a node inside a child that fills only through the children of an overlay. Fails on a
    /// condition that decides whether such a child fills and reads a node inside that child. Fails on a cycle of reads.
    pub fn new(nodes: &[Node], parents: &Parents) -> Result<Self, LayoutError> {
        let invalid_weight = nodes
            .iter()
            .find_map(|node| Some((node, node.invalid_weight?)));
        if let Some((node, cause)) = invalid_weight {
            let node = NodeName::of(node);
            return Err(LayoutError::Weight { node, cause });
        }
        if let Some(error) = nodes.iter().find_map(invalid_modifier) {
            return Err(error);
        }
        let mut graph = Self {
            references: references(nodes)?,
            ..Self::default()
        };
        let (sizes, moves): (Vec<_>, Vec<_>) = (graph.edges(nodes)?.into_iter())
            .partition(|edge| matches!(edge.used, Used::Length(_) | Used::Condition));
        graph.nodes_with_natural_width_read_by_placement = (moves.iter())
            .filter(|edge| edge.part == Part::NaturalWidth)
            .map(|edge| edge.source)
            .collect();
        graph.order_children(nodes, parents, &sizes)?;
        Positions::new(parents, &moves).check(nodes)?;
        Ok(graph)
    }

    pub fn node(&self, reference: &NodeRef) -> usize {
        self.references[&reference.key()]
    }

    /// The positions of the `count` children of `container` in the order in which it measures them.
    pub fn order(&self, container: usize, count: usize) -> Vec<usize> {
        (self.orders.get(&container)).map_or_else(|| (0..count).collect(), Vec::clone)
    }

    /// Whether a read of a size passes through `node`, so that the size of `node` is not a function
    /// of its constraints alone.
    pub fn is_crossed(&self, node: usize) -> bool {
        self.crossed.contains(&node)
    }

    /// The nodes outside `subtree` whose values are read by nodes inside `subtree`, each with the part read.
    pub fn read_into<'a>(
        &'a self,
        subtree: &'a BTreeSet<usize>,
    ) -> impl Iterator<Item = (usize, Part)> + 'a {
        (self.sizes.iter())
            .filter(|read| subtree.contains(&read.reader) && !subtree.contains(&read.source))
            .map(|read| (read.source, read.part))
    }

    /// The nodes outside `node` whose sizes the probe of the natural width of `node` reads, where a size or a condition
    /// reads that natural width.
    pub fn read_by_probe(&self, node: usize) -> impl Iterator<Item = usize> + '_ {
        (self.probes.get(&node).into_iter().flatten()).map(|read| read.source)
    }

    /// The nodes whose natural width is read by an offset or a position after measuring.
    pub fn nodes_with_natural_width_read_by_placement(&self) -> &BTreeSet<usize> {
        &self.nodes_with_natural_width_read_by_placement
    }

    fn edges(&self, nodes: &[Node]) -> Result<Vec<Edge>, LayoutError> {
        let mut edges = Vec::new();
        for (reader, node) in nodes.iter().enumerate() {
            let values = node
                .modifier
                .iter()
                .filter_map(ModifierElement::layout_value);
            for value in values {
                for read in value.reads() {
                    let Read::Node(reference, part) = read else {
                        continue;
                    };
                    let used = value.used();
                    let Some(&source) = self.references.get(&reference.key()) else {
                        let node = NodeName::of(&nodes[reader]);
                        return Err(LayoutError::UnsetNodeRef(NodeValue { node, used }));
                    };
                    let part = *part;
                    edges.push(Edge {
                        reader,
                        used,
                        source,
                        part,
                    });
                }
            }
        }
        Ok(edges)
    }

    /// Moves each read of a size in `reads` up to the nearest container of both nodes, and orders the children of
    /// each such container so that the source of each read is measured before its reader. A read of a natural width
    /// moves up as the reads that the probe of that width makes from inside its node to the nodes outside it, because
    /// the probe measures the node itself.
    fn order_children(
        &mut self,
        nodes: &[Node],
        parents: &Parents,
        reads: &[Edge],
    ) -> Result<(), LayoutError> {
        let mut probes = Probes {
            parents,
            reads,
            known: BTreeMap::new(),
            open: Vec::new(),
        };
        let mut through_probes = Vec::new();
        for &read in reads.iter().filter(|read| read.part == Part::NaturalWidth) {
            self.sizes.push(read);
            for inner in probes.reads_outside(nodes, read)? {
                through_probes.push(Edge {
                    source: inner.source,
                    part: inner.part,
                    ..read
                });
            }
        }
        self.probes = probes.known;
        let direct = (reads.iter().copied()).filter(|read| read.part != Part::NaturalWidth);
        let mut lifted: BTreeMap<usize, Vec<Lifted>> = BTreeMap::new();
        for edge in direct.chain(through_probes) {
            let (container, read) = self.lift(nodes, parents, edge)?;
            lifted.entry(container).or_default().push(read);
        }
        let order = |(container, reads): (usize, Vec<Lifted>)| {
            order(nodes, nodes[container].children.len(), &reads).map(|order| (container, order))
        };
        self.orders = lifted.into_iter().map(order).collect::<Result<_, _>>()?;
        Ok(())
    }

    /// Moves the read of a size `edge` up to the nearest container of both nodes, adds the nodes on the way to
    /// `crossed`, and returns the container.
    fn lift(
        &mut self,
        nodes: &[Node],
        parents: &Parents,
        edge: Edge,
    ) -> Result<(usize, Lifted), LayoutError> {
        let (source, reader) = (parents.chain(edge.source), parents.chain(edge.reader));
        let shared = (source.iter().rev().zip(reader.iter().rev()))
            .take_while(|(a, b)| a.node == b.node)
            .count();
        let below = |chain: &[Parent]| chain.len() - shared;
        let (from, to) = match (source.get(below(&source)), reader.get(below(&reader))) {
            (Some(&from), Some(&to)) if from.position != to.position => (from, to),
            _ => return Err(LayoutError::RelativeSizeRead(layout_read(nodes, &edge))),
        };
        let child_at = |parent: Parent| nodes[parent.node].children[parent.position];
        let weighted = |child: Parent| weight(nodes, child.node, child_at(child), true).is_some();
        // The container resolves the conditions of its children with a weight before it measures any of them, and the
        // conditions that decide whether a child fills before it measures that child. It measures a child that fills
        // only through hidden children as a child without a weight.
        let condition = edge.used == Used::Condition;
        let condition_of_weighted = condition && child_at(to) == edge.reader;
        let may_lose_weight = fills_through_children(nodes, to.node, child_at(to));
        let decided_fill = fill_decided_by(nodes, parents, edge.reader).filter(|_| condition);
        let reads_inside_decided = decided_fill
            .is_some_and(|child| parents.ancestry(edge.source).any(|above| above == child));
        let measured_later =
            weighted(from) && (!weighted(to) || condition_of_weighted || may_lose_weight);
        if measured_later || reads_inside_decided {
            return Err(LayoutError::WeightedReadOrder(layout_read(nodes, &edge)));
        }
        let path = |node: usize, chain: &[Parent]| {
            let between = chain[..below(chain)].iter().map(|parent| parent.node);
            std::iter::once(node).chain(between).collect::<Vec<_>>()
        };
        self.crossed.extend(path(edge.source, &source));
        self.crossed.extend(path(edge.reader, &reader));
        self.sizes.push(edge);
        let lifted = Lifted {
            from: from.position,
            to: to.position,
            edge,
        };
        Ok((from.node, lifted))
    }
}

/// The reads that the probes of natural widths make from inside the measured node to the nodes outside it. A natural
/// width read inside the measured node counts as the reads of its own probe.
struct Probes<'a> {
    parents: &'a Parents,
    reads: &'a [Edge],
    known: BTreeMap<usize, Vec<Edge>>,
    /// The reads of the natural widths whose probes are being followed, the outermost first.
    open: Vec<Edge>,
}

impl Probes<'_> {
    /// The reads that the probe of the natural width read by `read` makes to the nodes outside the source of `read`.
    /// Fails with the reads of a cycle where that probe reads the same natural width again.
    fn reads_outside(&mut self, nodes: &[Node], read: Edge) -> Result<Vec<Edge>, LayoutError> {
        let node = read.source;
        if let Some(known) = self.known.get(&node) {
            return Ok(known.clone());
        }
        if let Some(start) = self.open.iter().position(|open| open.source == node) {
            let mut cycle = self.open.split_off(start + 1);
            cycle.push(read);
            return Err(LayoutError::SizeCycle(layout_reads(nodes, &cycle)));
        }
        self.open.push(read);
        let (parents, reads) = (self.parents, self.reads);
        let inside = |other: usize| parents.ancestry(other).any(|above| above == node);
        let mut outside = Vec::new();
        for &inner in (reads.iter()).filter(|read| made_by_probe(parents, node, **read)) {
            let made = if inner.part == Part::NaturalWidth {
                self.reads_outside(nodes, inner)?
            } else {
                vec![inner]
            };
            for found in made.into_iter().filter(|found| !inside(found.source)) {
                if !outside.contains(&found) {
                    outside.push(found);
                }
            }
        }
        let _followed = self.open.pop();
        let _none_for_a_new_node = self.known.insert(node, outside.clone());
        Ok(outside)
    }
}

/// Whether the probe of the natural width of `node` makes `read`. The probe makes the reads from inside the node, apart
/// from the width set on `node` and the condition of `node`, which the parent of `node` resolves.
fn made_by_probe(parents: &Parents, node: usize, read: Edge) -> bool {
    let inside = parents.ancestry(read.reader).any(|above| above == node);
    inside && (read.reader != node || read.used == Used::Length(Axis::Vertical))
}

/// The error of the first value in the modifier of `node` that is not valid.
fn invalid_modifier(node: &Node) -> Option<LayoutError> {
    node.modifier.iter().find_map(|element| match element {
        ModifierElement::Invalid { value, cause } => Some(LayoutError::Modifier {
            node: NodeName::of(node),
            value: *value,
            cause: *cause,
        }),
        _ => None,
    })
}

/// The index of the node on which each reference is set. Fails on a reference set on two nodes.
fn references(nodes: &[Node]) -> Result<BTreeMap<NodeRefKey, usize>, LayoutError> {
    let mut references = BTreeMap::new();
    for (index, node) in nodes.iter().enumerate() {
        for reference in &node.references {
            if let Some(other) = references.insert(reference.key(), index)
                && other != index
            {
                let (first, second) = (NodeName::of(&nodes[other]), NodeName::of(node));
                return Err(LayoutError::DuplicateNodeRef { first, second });
            }
        }
    }
    Ok(references)
}

/// The positions of `count` children, sorted so that the child with the source of each read in `reads` comes
/// before the child with the reader. Children that no read orders come in the order of their positions. Fails with
/// the reads of a cycle when no such order exists.
fn order(nodes: &[Node], count: usize, reads: &[Lifted]) -> Result<Vec<usize>, LayoutError> {
    let mut incoming = vec![0; count];
    for read in reads {
        incoming[read.to] += 1;
    }
    let mut unblocked: BinaryHeap<Reverse<usize>> = (0..count)
        .filter(|p| incoming[*p] == 0)
        .map(Reverse)
        .collect();
    let mut order = Vec::with_capacity(count);
    while let Some(Reverse(position)) = unblocked.pop() {
        order.push(position);
        for read in reads.iter().filter(|read| read.from == position) {
            incoming[read.to] -= 1;
            if incoming[read.to] == 0 {
                unblocked.push(Reverse(read.to));
            }
        }
    }
    if order.len() < count {
        let cycle = cycle(reads, &incoming);
        return Err(LayoutError::SizeCycle(layout_reads(nodes, &cycle)));
    }
    Ok(order)
}

/// The reads that form a cycle among the children that still have `incoming` reads after the sort. In the result,
/// the reader of each read is the source of the read before it.
fn cycle(reads: &[Lifted], incoming: &[usize]) -> Vec<Edge> {
    let left = |position: usize| incoming[position] > 0;
    let mut visited: Vec<usize> = (0..incoming.len()).find(|p| left(*p)).into_iter().collect();
    let mut edges = Vec::new();
    while let Some(&at) = visited.last()
        && let Some(read) = reads.iter().find(|read| read.to == at && left(read.from))
    {
        edges.push(read.edge);
        if let Some(first) = visited.iter().position(|p| *p == read.from) {
            return edges.split_off(first);
        }
        visited.push(read.from);
    }
    edges
}

/// Checks that no offset and no position of a node reads bounds that move with that offset or position, such as the
/// bounds of the node itself or of a node inside it.
struct Positions<'a> {
    parents: &'a Parents,
    reads: BTreeMap<usize, Vec<Edge>>,
    visits: Vec<Visit>,
    /// The nodes on the current path of the search, each with the read that leads to the next node on the path, or
    /// `None` where the next node is the parent.
    path: Vec<(usize, Option<Edge>)>,
}

#[derive(Clone, Copy, PartialEq)]
enum Visit {
    New,
    Open,
    Done,
}

impl<'a> Positions<'a> {
    /// `moves` are the reads made by offsets and positions. Only a read of bounds changes with an offset or a
    /// position, so `reads` contains only those reads.
    fn new(parents: &'a Parents, moves: &[Edge]) -> Self {
        let mut reads: BTreeMap<usize, Vec<Edge>> = BTreeMap::new();
        for &edge in moves.iter().filter(|edge| edge.part == Part::Bounds) {
            reads.entry(edge.reader).or_default().push(edge);
        }
        Self {
            visits: vec![Visit::New; parents.len()],
            parents,
            reads,
            path: Vec::new(),
        }
    }

    fn check(mut self, nodes: &[Node]) -> Result<(), LayoutError> {
        let readers: Vec<usize> = self.reads.keys().copied().collect();
        for reader in readers {
            if self.visits[reader] == Visit::New {
                self.visit(nodes, reader)?;
            }
        }
        Ok(())
    }

    /// Visits `node` and the nodes on which the position of `node` depends: its parent, and the sources of the bounds
    /// read by its offsets and positions. Fails with the reads on the path when the search comes back to a node on
    /// the path.
    fn visit(&mut self, nodes: &[Node], node: usize) -> Result<(), LayoutError> {
        self.visits[node] = Visit::Open;
        let parent = self.parents.node_of(node).map(|parent| (parent, None));
        let reads =
            (self.reads.get(&node).into_iter().flatten()).map(|edge| (edge.source, Some(*edge)));
        let next: Vec<_> = parent.into_iter().chain(reads).collect();
        let depth = self.path.len();
        for (dependency, edge) in next {
            self.path.push((node, edge));
            match self.visits[dependency] {
                Visit::New => self.visit(nodes, dependency)?,
                Visit::Open => {
                    let reads: Vec<Edge> = (self.path.iter())
                        .skip_while(|(node, _)| *node != dependency)
                        .filter_map(|(_, edge)| *edge)
                        .collect();
                    return Err(LayoutError::BoundsCycle(layout_reads(nodes, &reads)));
                }
                Visit::Done => {}
            }
            self.path.truncate(depth);
        }
        self.visits[node] = Visit::Done;
        Ok(())
    }
}

fn layout_read(nodes: &[Node], edge: &Edge) -> LayoutRead {
    let reader = NodeValue {
        node: NodeName::of(&nodes[edge.reader]),
        used: edge.used,
    };
    LayoutRead {
        reader,
        source: NodeName::of(&nodes[edge.source]),
        part: edge.part,
    }
}

fn layout_reads(nodes: &[Node], edges: &[Edge]) -> LayoutReads {
    LayoutReads(edges.iter().map(|edge| layout_read(nodes, edge)).collect())
}
