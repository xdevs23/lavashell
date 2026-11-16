// SPDX-License-Identifier: GPL-3.0-only
mod children;
mod content_sizes;
mod pass;
mod placement;
mod weights;

use super::failure::Failed;
use super::frame_error::{ComponentError, NodeName};
use super::graph::Graph;
use super::{TextMeasurer, View};
use crate::geometry::{Constraints, Point, Rect, Size, Span};
use crate::protected_text::ProtectedInput;
use crate::reactive::Phase;
use crate::scene::TextStyle;
use crate::ui::{
    ContentSizes, Deferred, FieldContent, IconSize, Measured, ModifierElement, Node, NodeKind,
    NodeRef,
};
use crate::units::{Dp, NonNegativeDp, NumberError};
use pass::Pass;
pub(super) use weights::{fill_decided_by, fills_through_children, weight};

#[derive(Default, Clone, PartialEq)]
pub(super) struct Layout {
    pub size: Size,
    pub content: Size,
    pub layers: Vec<Size>,
    pub starts: Vec<Point>,
    pub offset: Point,
    pub children: Vec<Point>,
    pub rect: Rect,
    pub line_step: Dp,
    pub visibility: Visibility,
    pub natural_width: Option<Dp>,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum Visibility {
    #[default]
    Shown,
    Hidden,
}

impl Layout {
    pub fn innermost_layer(&self) -> Size {
        self.layers.last().copied().unwrap_or(self.size)
    }

    pub fn content_start(&self) -> Point {
        self.starts.last().copied().unwrap_or_default()
    }
}

pub(super) fn shown_children<'a>(
    nodes: &'a [Node],
    layout: &'a [Layout],
    index: usize,
) -> impl Iterator<Item = (usize, usize)> + Clone + 'a {
    (nodes[index].children.iter().copied().enumerate())
        .filter(move |&(_, child)| layout[child].visibility == Visibility::Shown)
}

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
    pub fn size_that_fits(
        &mut self,
        proposal: Proposal,
        text: &mut impl TextMeasurer,
    ) -> Result<Size, NumberError> {
        let constraints = proposal.constraints()?;
        if self.nodes.is_empty() {
            self.compose_root();
        }
        let mut measuring = Measuring::new(&self.nodes, &self.graph, Pass::probe(), text);
        Ok(measuring.measure(0, 0, constraints))
    }

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

pub(super) struct Measuring<'a, T> {
    pub(super) nodes: &'a [Node],
    pub(super) graph: &'a Graph,
    pass: Pass<'a>,
    text: &'a mut T,
    pub(super) layout: Vec<Layout>,
    pub(super) filled: Vec<bool>,
    measured: Vec<(usize, Option<usize>)>,
    owner: Option<usize>,
    components: Vec<(usize, Constraints)>,
    content_sizes: Vec<Option<ContentSizes>>,
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
        let element = self.nodes[index].modifier.get(step).cloned();
        let size = match element {
            Some(element) => self.measure_modifier(index, step, element, c),
            None => c.constrain(self.measure_kind(index, c)),
        };
        self.record_layer(index, step, size);
        size
    }

    fn measure_modifier(
        &mut self,
        index: usize,
        step: usize,
        element: ModifierElement,
        c: Constraints,
    ) -> Size {
        match element {
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
            ModifierElement::Width(_)
            | ModifierElement::Background(_)
            | ModifierElement::Border { .. }
            | ModifierElement::Clip(_)
            | ModifierElement::ClipRect(..)
            | ModifierElement::Effect { .. }
            | ModifierElement::Backdrop { .. }
            | ModifierElement::Shadow { .. }
            | ModifierElement::Outline { .. }
            | ModifierElement::Material(_)
            | ModifierElement::MaterialGroup { .. }
            | ModifierElement::Island(_)
            | ModifierElement::Opacity(_)
            | ModifierElement::Offset(_)
            | ModifierElement::Position(_)
            | ModifierElement::ShownWhen(_)
            | ModifierElement::AnimatedOffset(_)
            | ModifierElement::AnimatedOpacity(_)
            | ModifierElement::AnimatedBackground(_)
            | ModifierElement::FollowScroll { .. }
            | ModifierElement::Invalid { .. } => self.measure(index, step + 1, c),
        }
    }

    fn measure_kind(&mut self, index: usize, c: Constraints) -> Size {
        let nodes = self.nodes;
        match &nodes[index].kind {
            NodeKind::Text(value, style) => {
                let width = self.text_width(c.width, |text| text.min_width(value, style));
                self.measure_text(index, width, |text| text.measure(value, style, width))
            }
            NodeKind::TextInput {
                content,
                placeholder,
                style,
                ..
            } => self.measure_field(index, content, placeholder, style, c.width),
            NodeKind::Spacer(s) | NodeKind::Canvas { size: s, .. } => *s,
            NodeKind::Icon { size, .. } => {
                let length = match size {
                    IconSize::Line(style) => self.text.line_height(style),
                    IconSize::Fixed(fixed) => *fixed,
                };
                Size::new(length, length)
            }
            NodeKind::Container(arrangement) => self.measure_children(index, *arrangement, c),
        }
    }

    fn record_layer(&mut self, index: usize, step: usize, size: Size) {
        if self.layout[index].layers.len() <= step {
            self.layout[index].layers.resize(step + 1, Size::ZERO);
        }
        self.layout[index].layers[step] = size;
        if step == 0 {
            if let Some(style) = self.nodes[index].scroll_line() {
                self.layout[index].line_step = self.text.line_height(style);
            }
            self.layout[index].size = size;
            self.filled[index] = true;
            self.measured.push((index, self.owner));
        }
    }

    fn measure_field(
        &mut self,
        index: usize,
        content: &FieldContent,
        placeholder: &str,
        style: &TextStyle<Dp>,
        width: Span,
    ) -> Size {
        let style = &content.line_style(style);
        if let Some(private) = content.protected().and_then(ProtectedInput::presentation) {
            let width = self.text_width(width, |text| text.private_min_width(&private, style));
            self.measure_text(index, width, |text| {
                text.private_measure(&private, style, width)
            })
        } else {
            let value = content.shown();
            let shown = if value.is_empty() {
                placeholder
            } else {
                &value
            };
            let width = self.text_width(width, |text| text.min_width(shown, style));
            self.measure_text(index, width, |text| text.measure(shown, style, width))
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
