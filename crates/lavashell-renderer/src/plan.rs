// SPDX-License-Identifier: GPL-3.0-only
//! The plan for painting a scene tree at one presentation time: the steps in paint order, each with its device
//! pixels, the destination it paints into and the cached copy it uses.

use crate::Painter;
use crate::destination::Destination;
use crate::paint::analysis::Analysis;
use crate::paint::{dp_of, rounded, skmatrix, skrect};
use crate::scalar::scalar;
use lavashell_core::prelude::*;
use lavashell_core::retained::Extent;
use lavashell_core::scene::Affine;
use lavashell_core::scene::tree::{
    GroupValues, NamespaceId, NodeKey, Packet, PaintVisitor, PlacedLeaf, Presented,
};
use lavashell_core::surface::SurfaceId;
use skia_safe::{Canvas, IRect, Matrix, Path, RoundOut};
use std::collections::HashSet;
use std::rc::Rc;

mod caching;

pub(crate) use caching::{Cached, DeviceClip, Placement};

pub(crate) struct Plan<'a> {
    pub steps: Vec<Planned<'a>>,
    pub all_leaves_cacheable: bool,
    /// Whether some leaf samples pixels outside every extent declared in its footprint.
    pub unknown_sampling: bool,
    pub pass: u64,
}

/// What a pass over a plan repaints.
#[derive(Clone, Copy)]
pub(crate) enum Area {
    Everything,
    Pixels(IRect),
}

impl Area {
    /// Whether painting `extent` changes pixels that the pass repaints.
    pub fn repaints(self, extent: Option<IRect>) -> bool {
        extent.is_some_and(|extent| match self {
            Self::Everything => true,
            Self::Pixels(area) => IRect::intersects(&extent, &area),
        })
    }
}

pub(crate) enum Planned<'a> {
    Group {
        group: PlannedGroup,
        steps: Vec<Planned<'a>>,
    },
    Graft(Vec<Planned<'a>>),
    Leaf(Leaf<'a>),
}

pub(crate) struct PlannedGroup {
    pub values: GroupValues,
    pub destination: Destination,
    pub reads_parent: bool,
    pub device_extent: Option<IRect>,
}

pub(crate) struct Leaf<'a> {
    pub key: NodeKey,
    pub packet: &'a Packet,
    pub analysis: Rc<Analysis>,
    pub destination: Destination,
    pub device_extent: Option<IRect>,
    pub cached: Cached,
}

/// The state that the walk sets when it enters a group or a graft, and restores when it leaves.
struct GroupFrame<'a> {
    /// The group that the frame opened, or `None` for a graft and for the tree.
    group: Option<PlannedGroup>,
    matrix: Matrix,
    bounds: Option<IRect>,
    clips: usize,
    destination: Destination,
    device_extent: Option<IRect>,
    reads_parent: bool,
    steps: Vec<Planned<'a>>,
}

struct Walk<'p, 'a> {
    painter: &'p mut Painter,
    surface: SurfaceId,
    bytes_per_pixel: usize,
    clips: Vec<DeviceClip>,
    frame: GroupFrame<'a>,
    all_leaves_cacheable: bool,
    unknown_sampling: bool,
    shown: HashSet<NodeKey>,
}

impl Painter {
    /// `shape` is the outline of the surface. Mapped by the matrix of `canvas`, the outline clips the tree.
    pub(crate) fn plan<'a>(
        &mut self,
        canvas: &Canvas,
        tree: Presented<'a>,
        density: Density,
        shape: Option<&Path>,
    ) -> Plan<'a> {
        let pass = self.packet_cache.begin();
        let mut matrix = canvas.local_to_device_as_3x3();
        let mut bounds = canvas.device_clip_bounds();
        let mut clips = Vec::new();
        if let Some(shape) = shape {
            bounds = within(bounds, matrix.map_rect(shape.bounds()).0);
            clips.extend(DeviceClip::outline(shape, &matrix));
        }
        match scalar(density.get()) {
            Ok(scale) => {
                let _chained_matrix = matrix.pre_scale((scale, scale), None);
            }
            Err(_) => bounds = None,
        }
        let frame = GroupFrame {
            group: None,
            matrix,
            bounds,
            clips: 0,
            destination: self.compositing_destination(),
            device_extent: None,
            reads_parent: false,
            steps: Vec::new(),
        };
        let surface = self.effect_planner.current_surface();
        self.analyses.begin(surface);
        let mut walk = Walk {
            painter: self,
            surface,
            bytes_per_pixel: canvas.image_info().bytes_per_pixel(),
            clips,
            frame,
            all_leaves_cacheable: true,
            unknown_sampling: false,
            shown: HashSet::new(),
        };
        tree.paint_order().walk(&mut walk);
        let Walk {
            frame,
            all_leaves_cacheable,
            unknown_sampling,
            shown,
            ..
        } = walk;
        let mut steps = frame.steps;
        caching::fit(&mut steps, self.packet_cache.budget(canvas));
        self.analyses.end(surface);
        self.packet_cache.retain_shown(surface, &shown);
        Plan {
            steps,
            all_leaves_cacheable,
            unknown_sampling,
            pass,
        }
    }

    fn plan_leaf<'a>(
        &mut self,
        surface: SurfaceId,
        key: NodeKey,
        packet: &'a Packet,
        destination: Destination,
        placement: &Placement<'_>,
    ) -> Leaf<'a> {
        let analysis = self.analyses.get(surface, packet, &self.effect_registry);
        let footprint = packet.footprint();
        let device_extent = [footprint.output, footprint.backdrop]
            .into_iter()
            .flatten()
            .filter_map(|extent| device_pixels(extent, placement.matrix, placement.bounds))
            .reduce(|a, b| IRect::join(&a, &b));
        let exact = footprint.cacheable
            && !analysis.reads_parent()
            && !analysis.has_effects()
            && destination == Destination::Surface;
        let cover = device_pixel(placement.matrix).and_then(|pixel| footprint.opaque(pixel));
        let cached = match cover.filter(|_| exact) {
            Some(cover) => (self.packet_cache).cached(surface, key, packet, cover, placement),
            None => Cached::No,
        };
        Leaf {
            key,
            packet,
            analysis,
            destination,
            device_extent: device_extent.filter(|_| footprint.output.is_some()),
            cached,
        }
    }
}

impl<'a> PaintVisitor<'a> for Walk<'_, 'a> {
    type Saved = GroupFrame<'a>;

    /// A group whose transform or clip lies beyond the range of Skia scalars shows nothing.
    fn group(&mut self, _: NodeKey, values: GroupValues, _: Affine) -> GroupFrame<'a> {
        let mut inner = self.frame.inner(self.clips.len());
        match skmatrix(&values.transform) {
            Ok(transform) => inner.matrix = Matrix::concat(&inner.matrix, &transform),
            Err(_) => inner.bounds = None,
        }
        if let Some(clip) = values.clip {
            match (skrect(clip.rect), rounded(&clip)) {
                (Ok(rect), Ok(rounded)) => {
                    inner.bounds = within(inner.bounds, inner.matrix.map_rect(rect).0);
                    (self.clips).extend(DeviceClip::partial(rounded, &inner.matrix));
                }
                _ => inner.bounds = None,
            }
        }
        let destination = Destination::faded(values.opacity);
        inner.destination = inner.destination.enter(destination);
        inner.group = Some(PlannedGroup {
            values,
            destination,
            reads_parent: false,
            device_extent: None,
        });
        std::mem::replace(&mut self.frame, inner)
    }

    fn graft(&mut self, _: NodeKey, _: NamespaceId) -> GroupFrame<'a> {
        let inner = self.frame.inner(self.clips.len());
        std::mem::replace(&mut self.frame, inner)
    }

    fn leaf(&mut self, leaf: PlacedLeaf<'a>) {
        let placement = Placement {
            matrix: &self.frame.matrix,
            bounds: self.frame.bounds,
            clips: &self.clips,
            bytes_per_pixel: self.bytes_per_pixel,
        };
        let planned = (self.painter).plan_leaf(
            self.surface,
            leaf.key,
            leaf.packet,
            self.frame.destination,
            &placement,
        );
        self.all_leaves_cacheable &= leaf.packet.footprint().cacheable;
        self.unknown_sampling |= planned.analysis.unknown_sampling();
        self.frame.reads_parent |= planned.analysis.reads_parent();
        // An effect may paint or sample anywhere inside the clips.
        let painted = if planned.analysis.has_effects() {
            self.frame.bounds
        } else {
            planned.device_extent
        };
        self.frame.device_extent = join(self.frame.device_extent, painted);
        let _newly_shown = self.shown.insert(leaf.key);
        self.frame.steps.push(Planned::Leaf(planned));
    }

    fn exit(&mut self, outer: GroupFrame<'a>) {
        let inner = std::mem::replace(&mut self.frame, outer);
        self.clips.truncate(inner.clips);
        self.frame.device_extent = join(self.frame.device_extent, inner.device_extent);
        self.frame.reads_parent |= inner.reads_parent;
        let step = match inner.group {
            Some(group) => Planned::Group {
                group: PlannedGroup {
                    device_extent: inner.device_extent,
                    reads_parent: inner.reads_parent,
                    ..group
                },
                steps: inner.steps,
            },
            None => Planned::Graft(inner.steps),
        };
        self.frame.steps.push(step);
    }
}

impl GroupFrame<'_> {
    fn inner(&self, clips: usize) -> Self {
        Self {
            group: None,
            matrix: self.matrix,
            bounds: self.bounds,
            clips,
            destination: self.destination,
            device_extent: None,
            reads_parent: false,
            steps: Vec::new(),
        }
    }
}

impl Plan<'_> {
    /// The pixels repainted by a pass for `damage`. Widens `damage` until every layer and every leaf with a copy to
    /// fill lies either wholly inside it or outside it. A layer gets the size of the clip, so a pass repaints it whole.
    pub fn repaint(&self, mut damage: IRect) -> IRect {
        let mut uncut = Vec::new();
        let mut pending = Vec::new();
        every_step(&self.steps, &mut pending);
        loop {
            let before = damage;
            pending.retain(|step| match step.device_extent() {
                Some(extent) if IRect::intersects(&extent, &damage) => {
                    uncut.extend(step.uncut());
                    false
                }
                Some(_) => true,
                None => false,
            });
            uncut.retain(|rect| {
                let cut = IRect::intersects(rect, &damage);
                if cut {
                    damage = IRect::join(&damage, rect);
                }
                !cut
            });
            if damage == before {
                break;
            }
        }
        damage
    }
}

/// Collects `steps` and the steps inside them, in paint order.
fn every_step<'p, 'a>(steps: &'p [Planned<'a>], out: &mut Vec<&'p Planned<'a>>) {
    for step in steps {
        out.push(step);
        if let Planned::Group { steps, .. } | Planned::Graft(steps) = step {
            every_step(steps, out);
        }
    }
}

impl Planned<'_> {
    fn device_extent(&self) -> Option<IRect> {
        match self {
            Self::Group { group, .. } => group.device_extent,
            Self::Leaf(leaf) => leaf.device_extent,
            Self::Graft(_) => None,
        }
    }

    /// The device pixels of a layer, of a leaf that paints through layers or of a leaf with a copy to fill. A pass
    /// repaints these pixels either whole or not at all.
    fn uncut(&self) -> Option<IRect> {
        match self {
            Self::Group { group, .. } if group.destination == Destination::Layer => {
                group.device_extent
            }
            Self::Leaf(leaf) => match leaf.cached {
                Cached::Fill { .. } => leaf.device_extent,
                Cached::No => leaf.device_extent.filter(|_| leaf.analysis.layers),
                Cached::Hit { .. } => None,
            },
            Self::Group { .. } | Self::Graft(_) => None,
        }
    }
}

/// The device pixels of `bounds` inside the device rectangle `clip`.
fn within(bounds: Option<IRect>, clip: skia_safe::Rect) -> Option<IRect> {
    bounds.and_then(|bounds| IRect::intersect(&bounds, &clip.round_out()))
}

/// One device pixel in the coordinates that `matrix` maps to the device, along the axis on which it is longer,
/// when `matrix` only scales and translates.
fn device_pixel(matrix: &Matrix) -> Option<Dp> {
    let shorter = matrix.scale_x().abs().min(matrix.scale_y().abs());
    matrix.is_scale_translate().then(|| dp_of(shorter.recip()))
}

/// The device pixels of `extent` under `matrix` inside `bounds`, with one pixel of antialiased edge. An extent beyond
/// the range of Skia scalars covers all of `bounds`. The leaf still paints each of its draws that fits the range of
/// Skia scalars, and records the failure of every other draw.
fn device_pixels(extent: Extent, matrix: &Matrix, bounds: Option<IRect>) -> Option<IRect> {
    let bounds = bounds?;
    match extent {
        Extent::Within(rect) => match skrect(rect) {
            Ok(rect) => {
                let (mapped, _) = matrix.map_rect(rect);
                IRect::intersect(&mapped.with_outset((1.0, 1.0)).round_out(), &bounds)
            }
            Err(_) => Some(bounds),
        },
        Extent::Everywhere => Some(bounds),
    }
}

fn join(a: Option<IRect>, b: Option<IRect>) -> Option<IRect> {
    match (a, b) {
        (Some(a), Some(b)) => Some(IRect::join(&a, &b)),
        (a, b) => a.or(b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_extent_beyond_the_range_of_skia_scalars_reaches_all_of_the_bounds() {
        let bounds = IRect::from_wh(96, 48);
        let beyond = Rect::new(0.dp(), 0.dp(), Dp::new(f64::MAX), 1.dp());
        let pixels = device_pixels(
            Extent::Within(beyond),
            &Matrix::new_identity(),
            Some(bounds),
        );
        assert_eq!(pixels, Some(bounds));
    }
}
