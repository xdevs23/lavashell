// SPDX-License-Identifier: GPL-3.0-only
mod children;
mod content_sizes;
mod placement;
mod weights;

use super::failure::Failed;
use super::frame_error::{ComponentError, NodeName};
use super::graph::Graph;
use super::{
    BTreeMap, Constraints, Deferred, Dp, IconSize, ModifierElement, Node, NodeKind, NodeRef, Phase,
    Point, Rect, Size, Span, TextMeasurer, View, retained,
};
use crate::protected_text::ProtectedInput;
use crate::scene::TextStyle;
use crate::ui::{ContentSizes, FieldContent, Measured};
use crate::units::{NonNegativeDp, NumberError};
pub(super) use weights::{fill_decided_by, fills_through_children, weight};

/// The layout of a node, in the coordinates of the content of its parent.
#[derive(Default, Clone, PartialEq)]
pub(super) struct Layout {
    pub size: Size,
    pub content: Size,
    pub layers: Vec<Size>,
    /// Where each layer of the node starts, relative to where the parent places the node. Each element of the
    /// modifier of the node adds one layer. Padding, offsets and positions move the layers inside them.
    pub starts: Vec<Point>,
    /// How far the offsets and the positions of the node move it from where the parent places it.
    pub offset: Point,
    pub children: Vec<Point>,
    pub rect: Rect,
    /// One step of the arrow keys in a scrolling node: the height of one line of its text.
    pub line_step: Dp,
    pub visibility: Visibility,
    /// The width of the node at an unbounded proposal, before the width set on it, once a layout value reads it.
    pub natural_width: Option<Dp>,
}

/// Whether a node is shown, or hidden by a condition of the node or of a node around it.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum Visibility {
    #[default]
    Shown,
    Hidden,
}

impl Layout {
    /// The size of the innermost layer, inside every element of the modifier of the node, or the size of the node
    /// where no layer was measured.
    pub fn innermost_layer(&self) -> Size {
        self.layers.last().copied().unwrap_or(self.size)
    }

    /// Where the content starts, after the padding, the offsets and the positions of the node.
    pub fn content_start(&self) -> Point {
        self.starts.last().copied().unwrap_or_default()
    }
}

/// The position and the index of each shown child of the node at `index`.
pub(super) fn shown_children<'a>(
    nodes: &'a [Node],
    layout: &'a [Layout],
    index: usize,
) -> impl Iterator<Item = (usize, usize)> + Clone + 'a {
    (nodes[index].children.iter().copied().enumerate())
        .filter(move |&(_, child)| layout[child].visibility == Visibility::Shown)
}

/// A size proposed to a view when measuring it, per axis.
///
/// `Some(length)` is an upper bound along that axis. Within that bound, the content is as large as it needs to be,
/// but never smaller than its smallest size. The measured size can therefore be larger than the proposal, and
/// `Some(0.dp())` measures the smallest size of the view. `None` measures the content at its content size. `Some` of
/// an infinite length measures like `None`. Measuring fails on a negative length and on NaN.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Proposal {
    pub width: Option<Dp>,
    pub height: Option<Dp>,
}

impl Proposal {
    fn constraints(self) -> Result<Constraints, NumberError> {
        let span = |length: Option<Dp>| match length {
            Some(length) if length != Dp::INFINITY => {
                NonNegativeDp::try_new(length).map(|length| Span::offering(length.get()))
            }
            _ => Ok(Span::offering(Dp::INFINITY)),
        };
        Ok(Constraints {
            width: span(self.width)?,
            height: span(self.height)?,
        })
    }
}

impl View {
    /// Measures the view as last composed within `proposal`, without painting it. A view that never made a frame is
    /// composed first, at the root value of `VIEWPORT`. A component that fails in that composition has no nodes in the
    /// tree, and the next frame returns the failure. The measurement does not call the `on_measure` actions. Apart from
    /// that first composition, the measurement changes nothing that a later frame reads.
    ///
    /// # Errors
    ///
    /// Fails when a length of `proposal` is negative or not a number.
    pub fn size_that_fits(
        &mut self,
        proposal: Proposal,
        text: &mut impl TextMeasurer,
    ) -> Result<Size, NumberError> {
        let constraints = proposal.constraints()?;
        if self.nodes.is_empty() {
            self.compose_root();
        }
        let probe = Pass::Probe(MeasuredSizes::default());
        let mut measuring = Measuring::new(&self.nodes, &self.graph, probe, text);
        Ok(measuring.measure(0, 0, constraints))
    }

    /// Measures the view within `c` for a frame, sets the size of the root node as the size of the view, stores the
    /// layout and places the nodes. Then this function records the layouts of the components measured again, and
    /// reports the measured sizes to the `on_measure` and `on_content_sizes` actions and to the scroll states. Returns
    /// a failure around each node whose scroll state failed a scroll requested since the last layout.
    pub(super) fn lay_out(&mut self, c: Constraints, text: &mut impl TextMeasurer) -> Vec<Failed> {
        let frame = Pass::Frame(&self.retained);
        let mut measuring = Measuring::new(&self.nodes, &self.graph, frame, text);
        self.size = measuring.measure(0, 0, c);
        measuring.measure_natural_widths_read_by_placement();
        let Measuring {
            layout,
            measured,
            components,
            content_sizes,
            reused,
            ..
        } = measuring;
        self.layout = layout;
        self.place();
        self.stats.reused_layouts += reused;
        self.record_layouts(components);
        self.release_hidden_components();
        let mut failed = Vec::new();
        for (index, owner) in measured {
            let (node, layout) = (&self.nodes[index], &self.layout[index]);
            let report = || {
                let published = (node.scroll_state()).map_or(Ok(()), |state| {
                    state.publish(layout.innermost_layer(), layout.content, layout.line_step)
                });
                if let Some(action) = &node.measured {
                    action.borrow_mut()(layout.size);
                }
                if let (Some(action), Some(sizes)) = (&node.content_sizes, content_sizes[index]) {
                    action.borrow_mut()(sizes);
                }
                published
            };
            let published = match owner.and_then(|owner| self.nodes[owner].component.as_ref()) {
                Some(component) => component.scope.track_further(Phase::Layout, report),
                None => report(),
            };
            if let Err(error) = published {
                failed.push(Failed {
                    inside: self.parents.around(index),
                    cause: ComponentError::Scroll(error),
                });
            }
        }
        failed
    }
}

enum Pass<'a> {
    /// A measurement for a frame, with the layouts cached for components by earlier frames. The frame reuses a cached
    /// layout where it is still valid. Otherwise the frame measures the component again inside the scope of the
    /// component, which records the values read by the layout of the component.
    Frame(&'a retained::Retained),
    Probe(MeasuredSizes),
}

impl Pass<'_> {
    fn measured(&self, index: usize, c: Constraints) -> Option<Size> {
        match self {
            Self::Frame(_) => None,
            Self::Probe(sizes) => sizes.get(index, c),
        }
    }

    fn record(&mut self, index: usize, c: Constraints, size: Size) {
        if let Self::Probe(sizes) = self {
            sizes.insert(index, c, size);
        }
    }
}

/// The sizes measured for each node during one probe, with the constraints they were measured
/// under.
#[derive(Default)]
struct MeasuredSizes(BTreeMap<usize, Vec<(Constraints, Size)>>);

impl MeasuredSizes {
    fn get(&self, index: usize, c: Constraints) -> Option<Size> {
        (self.0.get(&index)?.iter()).find_map(|(measured, size)| (*measured == c).then_some(*size))
    }

    fn insert(&mut self, index: usize, c: Constraints, size: Size) {
        self.0.entry(index).or_default().push((c, size));
    }
}

/// Measures the nodes of a view into a new layout table and calls no callback.
pub(super) struct Measuring<'a, T> {
    pub(super) nodes: &'a [Node],
    pub(super) graph: &'a Graph,
    pass: Pass<'a>,
    text: &'a mut T,
    pub(super) layout: Vec<Layout>,
    /// Whether this pass filled in the layout of each node, by measuring the node or by copying its layout from the
    /// cache.
    pub(super) filled: Vec<bool>,
    /// The indices of the nodes measured, each after its descendants, with the index of the enclosing component that
    /// was measured again. The layout of that component depends on the values read by the reports of those nodes.
    measured: Vec<(usize, Option<usize>)>,
    owner: Option<usize>,
    /// The scoped components measured again, with the constraints they were measured under.
    components: Vec<(usize, Constraints)>,
    /// For each node with an `on_content_sizes` action, the size of the node without bounds and its smallest size.
    content_sizes: Vec<Option<ContentSizes>>,
    /// The node for which this measurement finds the natural width, so that the width set on that node is left out.
    natural_width_of: Option<usize>,
    pub(super) reused: u64,
}

impl<'a, T: TextMeasurer> Measuring<'a, T> {
    fn new(nodes: &'a [Node], graph: &'a Graph, pass: Pass<'a>, text: &'a mut T) -> Self {
        Self {
            nodes,
            graph,
            pass,
            text,
            layout: vec![Layout::default(); nodes.len()],
            filled: vec![false; nodes.len()],
            measured: Vec::new(),
            owner: None,
            components: Vec::new(),
            content_sizes: vec![None; nodes.len()],
            natural_width_of: None,
            reused: 0,
        }
    }

    fn measure(&mut self, index: usize, step: usize, c: Constraints) -> Size {
        let nodes = self.nodes;
        if step > 0 {
            return self.measure_inner(index, step, c);
        }
        if let Pass::Frame(cache) = self.pass
            && let Some(component) = &nodes[index].component
        {
            if let Some(size) = self.reuse_layout(cache, index, c) {
                return size;
            }
            let outer = self.owner.replace(index);
            let size = component.scope.track(Phase::Layout, || {
                self.measure_content_sizes(index);
                self.measure_inner(index, 0, c)
            });
            self.owner = outer;
            self.components.push((index, c));
            return size;
        }
        // The size of a node that a read of a size passes through is not a function of its constraints alone. A
        // size measured earlier under the same constraints can therefore be out of date.
        let reusable = !self.graph.is_crossed(index);
        if reusable && let Some(size) = self.pass.measured(index, c) {
            return size;
        }
        self.measure_content_sizes(index);
        let size = self.measure_inner(index, 0, c);
        if reusable {
            self.pass.record(index, c, size);
        }
        size
    }

    fn measure_inner(&mut self, index: usize, step: usize, c: Constraints) -> Size {
        let nodes = self.nodes;
        let element = nodes[index].modifier.get(step).cloned();
        let size = if let Some(e) = element {
            match e {
                ModifierElement::Padding(i) => {
                    let s = self.measure(index, step + 1, c.shrink(i));
                    c.constrain(Size::new(s.width + i.horizontal(), s.height + i.vertical()))
                }
                ModifierElement::Width(w) if self.natural_width_of != Some(index) => {
                    let width = c.width.at(self.length(&w));
                    self.measure(index, step + 1, Constraints { width, ..c })
                }
                ModifierElement::Height(h) => {
                    let height = c.height.at(self.length(&h));
                    self.measure(index, step + 1, Constraints { height, ..c })
                }
                ModifierElement::MeasurementLimit(limit) => {
                    let limited = Constraints {
                        width: c.width.limited(limit.width),
                        height: c.height.limited(limit.height),
                    };
                    let natural = self.measure(index, step + 1, limited);
                    c.constrain(natural)
                }
                ModifierElement::FillWidth => {
                    let width = c.width.filled();
                    self.measure(index, step + 1, Constraints { width, ..c })
                }
                ModifierElement::FillHeight => {
                    let height = c.height.filled();
                    self.measure(index, step + 1, Constraints { height, ..c })
                }
                ModifierElement::MaxSize(s) => {
                    let most = Constraints {
                        width: c.width.at_most(s.width),
                        height: c.height.at_most(s.height),
                    };
                    self.measure(index, step + 1, most)
                }
                ModifierElement::MinSize(s) => {
                    let least = Constraints {
                        width: c.width.at_least(s.width),
                        height: c.height.at_least(s.height),
                    };
                    self.measure(index, step + 1, least)
                }
                _ => self.measure(index, step + 1, c),
            }
        } else {
            let desired = match &nodes[index].kind {
                NodeKind::Text(value, style) => {
                    let width = self.text_width(c.width, |text| text.min_width(value, style));
                    self.text.measure(value, style, width)
                }
                NodeKind::TextInput {
                    content,
                    placeholder,
                    style,
                    ..
                } => self.measure_field(content, placeholder, style, c.width),
                NodeKind::Spacer(s) | NodeKind::Canvas { size: s, .. } => *s,
                NodeKind::Icon { size, .. } => {
                    let length = match size {
                        IconSize::Line(style) => self.text.line_height(style),
                        IconSize::Fixed(fixed) => *fixed,
                    };
                    Size::new(length, length)
                }
                NodeKind::Container(arrangement) => self.measure_children(index, *arrangement, c),
            };
            c.constrain(desired)
        };
        if self.layout[index].layers.len() <= step {
            self.layout[index].layers.resize(step + 1, Size::ZERO);
        }
        self.layout[index].layers[step] = size;
        if step == 0 {
            if let Some(style) = nodes[index].scroll_line() {
                self.layout[index].line_step = self.text.line_height(style);
            }
            self.layout[index].size = size;
            self.filled[index] = true;
            self.measured.push((index, self.owner));
        }
        size
    }

    /// The size of a field that shows `content`, or `placeholder` where the content is empty.
    fn measure_field(
        &mut self,
        content: &FieldContent,
        placeholder: &str,
        style: &TextStyle<Dp>,
        width: Span,
    ) -> Size {
        let style = &content.line_style(style);
        if let Some(private) = content.protected().and_then(ProtectedInput::presentation) {
            let width = self.text_width(width, |text| text.private_min_width(&private, style));
            self.text.private_measure(&private, style, width)
        } else {
            let value = content.shown();
            let shown = if value.is_empty() {
                placeholder
            } else {
                &value
            };
            let width = self.text_width(width, |text| text.min_width(shown, style));
            self.text.measure(shown, style, width)
        }
    }

    fn text_width(&mut self, width: Span, min_width: impl FnOnce(&mut T) -> Dp) -> Dp {
        if width.allows_more() {
            width.clamp(width.proposed().max(min_width(self.text)))
        } else {
            width.proposed()
        }
    }

    fn length(&mut self, value: &Deferred<Dp>) -> Dp {
        value.resolve(self).max(Dp::ZERO)
    }
}

impl<T: TextMeasurer> Measured for Measuring<'_, T> {
    fn size(&mut self, node: &NodeRef) -> Size {
        let index = self.graph.node(node);
        debug_assert!(
            self.filled[index],
            "the size of {} is read before it is measured",
            NodeName::of(&self.nodes[index])
        );
        self.layout[index].size
    }

    fn natural_width(&mut self, node: &NodeRef) -> Dp {
        let index = self.graph.node(node);
        self.natural_width_at(index)
    }
}
