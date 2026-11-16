// SPDX-License-Identifier: GPL-3.0-only

use crate::Painter;
use crate::destination::{Destination, GroupOpacity};
use crate::failure::{DrawFailure, Failures};
use crate::paint::analysis::Analysis;
use crate::paint::{dp_of, rounded, skmatrix, skrect};
use crate::scalar::scalar;
use lavashell_core::prelude::*;
use lavashell_core::retained::{Extent, UnbalancedRestore};
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
    pub surface: SurfaceId,
    pub steps: Vec<Planned<'a>>,
    pub all_leaves_cacheable: bool,
    pub unknown_sampling: bool,
    pub pass: u64,
}

#[derive(Clone, Copy)]
pub(crate) enum Area {
    Everything,
    Pixels(IRect),
}

impl Area {
    pub fn repainted(self, extent: Option<IRect>) -> Option<IRect> {
        extent.filter(|extent| match self {
            Self::Everything => true,
            Self::Pixels(area) => IRect::intersects(extent, &area),
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
    pub opacity: GroupOpacity,
    pub reads_parent: bool,
    pub device_extent: Option<IRect>,
}

pub(crate) struct Leaf<'a> {
    pub surface: SurfaceId,
    pub key: NodeKey,
    pub packet: &'a Packet,
    pub analysis: Rc<Analysis>,
    pub destination: Destination,
    pub device_extent: Option<IRect>,
    pub cached: Cached,
}

struct GroupFrame<'a> {
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
    pub(crate) fn plan<'a>(
        &mut self,
        canvas: &Canvas,
        surface: SurfaceId,
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
                let _matrix_returned_for_chaining = matrix.pre_scale((scale, scale), None);
            }
            Err(failure) => {
                self.failures.record(failure);
                bounds = None;
            }
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
        caching::fit(
            &mut steps,
            (self.packet_cache).budget(canvas, &self.resources.held),
        );
        self.analyses.end(surface);
        self.packet_cache.retain_shown(surface, &shown);
        Plan {
            surface,
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
    ) -> Result<Leaf<'a>, UnbalancedRestore> {
        let analysis = self.analyses.get(surface, packet, &self.effect_registry)?;
        let footprint = packet.footprint();
        let device_extent = [footprint.output, footprint.backdrop]
            .into_iter()
            .flatten()
            .filter_map(|extent| {
                device_pixels(extent, placement.matrix, placement.bounds, &self.failures)
            })
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
        Ok(Leaf {
            surface,
            key,
            packet,
            analysis,
            destination,
            device_extent: device_extent.filter(|_| footprint.output.is_some()),
            cached,
        })
    }
}

impl<'a> PaintVisitor<'a> for Walk<'_, 'a> {
    type Saved = GroupFrame<'a>;

    fn group(&mut self, _: NodeKey, values: GroupValues, _: Affine) -> GroupFrame<'a> {
        let mut inner = self.frame.inner(self.clips.len());
        match skmatrix(&values.transform) {
            Ok(transform) => inner.matrix = Matrix::concat(&inner.matrix, &transform),
            Err(failure) => {
                self.painter.failures.record(failure);
                inner.bounds = None;
            }
        }
        if let Some(clip) = values.clip {
            match skrect(clip.rect).and_then(|rect| Ok((rect, rounded(&clip)?))) {
                Ok((rect, rounded)) => {
                    inner.bounds = within(inner.bounds, inner.matrix.map_rect(rect).0);
                    (self.clips).extend(DeviceClip::partial(rounded, &inner.matrix));
                }
                Err(failure) => {
                    self.painter.failures.record(failure);
                    inner.bounds = None;
                }
            }
        }
        let opacity = GroupOpacity::new(values.opacity);
        inner.destination = inner.destination.inside(opacity.destination());
        inner.group = Some(PlannedGroup {
            values,
            opacity,
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
        let planned = match (self.painter).plan_leaf(
            self.surface,
            leaf.key,
            leaf.packet,
            self.frame.destination,
            &placement,
        ) {
            Ok(planned) => planned,
            Err(cause) => {
                (self.painter.failures).record_caused(DrawFailure::UnbalancedRestore, &cause);
                return;
            }
        };
        self.all_leaves_cacheable &= leaf.packet.footprint().cacheable;
        self.unknown_sampling |= planned.analysis.unknown_sampling();
        self.frame.reads_parent |= planned.analysis.reads_parent();
        let painted = if planned.analysis.has_effects() {
            self.frame.bounds
        } else {
            planned.device_extent
        };
        self.frame.device_extent = join(self.frame.device_extent, painted);
        let _whether_newly_shown_is_not_needed = self.shown.insert(leaf.key);
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

    fn uncut(&self) -> Option<IRect> {
        match self {
            Self::Group { group, .. } if group.opacity.destination() == Destination::Layer => {
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

fn within(bounds: Option<IRect>, clip: skia_safe::Rect) -> Option<IRect> {
    bounds.and_then(|bounds| IRect::intersect(&bounds, &clip.round_out()))
}

fn device_pixel(matrix: &Matrix) -> Option<Dp> {
    let shorter = matrix.scale_x().abs().min(matrix.scale_y().abs());
    matrix.is_scale_translate().then(|| dp_of(shorter.recip()))
}

fn device_pixels(
    extent: Extent,
    matrix: &Matrix,
    bounds: Option<IRect>,
    failures: &Failures,
) -> Option<IRect> {
    let bounds = bounds?;
    match extent {
        Extent::Within(rect) => match skrect(rect) {
            Ok(rect) => {
                let (mapped, _bounds_cover_every_shape) = matrix.map_rect(rect);
                IRect::intersect(&mapped.with_outset((1.0, 1.0)).round_out(), &bounds)
            }
            Err(failure) => {
                failures.record(failure);
                Some(bounds)
            }
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
            &Failures::default(),
        );
        assert_eq!(pixels, Some(bounds));
    }

    #[test]
    fn a_pass_repaints_an_extent_only_where_the_extent_meets_the_area_of_the_pass() {
        let extent = IRect::new(10, 10, 20, 20);
        let meeting = Area::Pixels(IRect::new(15, 15, 30, 30));
        let adjacent = Area::Pixels(IRect::new(20, 10, 30, 20));
        assert_eq!(Area::Everything.repainted(Some(extent)), Some(extent));
        assert_eq!(meeting.repainted(Some(extent)), Some(extent));
        assert_eq!(adjacent.repainted(Some(extent)), None);
        assert_eq!(Area::Everything.repainted(None), None);
    }
}
