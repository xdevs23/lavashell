// SPDX-License-Identifier: GPL-3.0-only

use super::{Leaf, Planned};
use crate::packet_cache::PacketCacheKey;
use crate::paint::skrect;
use crate::raster::pixel_bytes;
use lavashell_core::prelude::*;
use skia_safe::{Contains, IRect, Image, Matrix, Path, RRect};

pub(crate) enum Cached {
    No,
    Fill {
        key: PacketCacheKey,
        pixels: IRect,
        bytes: usize,
    },
    Hit {
        image: Image,
        pixels: IRect,
        bytes: usize,
    },
}

pub(crate) enum DeviceClip {
    Rounded(RRect),
    Opaque,
}

impl DeviceClip {
    pub fn partial(clip: RRect, matrix: &Matrix) -> Option<Self> {
        let Some(device) = clip.transform(matrix) else {
            return Some(Self::Opaque);
        };
        let whole = device.is_rect() && on_whole_pixels(*device.rect()).is_some();
        (!whole).then_some(Self::Rounded(device))
    }

    pub fn outline(shape: &Path, matrix: &Matrix) -> Option<Self> {
        let area =
            (shape.is_rrect()).or_else(|| shape.is_rect().map(|(rect, ..)| RRect::new_rect(rect)));
        match area {
            Some(area) => Self::partial(area, matrix),
            None => Some(Self::Opaque),
        }
    }

    fn contains(&self, rect: skia_safe::Rect) -> bool {
        match self {
            Self::Rounded(clip) => clip.contains(rect),
            Self::Opaque => false,
        }
    }
}

pub(crate) struct Placement<'a> {
    pub matrix: &'a Matrix,
    pub bounds: Option<IRect>,
    pub clips: &'a [DeviceClip],
    pub bytes_per_pixel: usize,
}

impl Placement<'_> {
    pub fn pixels(&self, cover: Rect) -> Option<IRect> {
        let pixels =
            whole_pixels(self.matrix, cover).filter(|_| self.matrix.is_scale_translate())?;
        let inside = skia_safe::Rect::from(pixels).with_outset((1.0, 1.0));
        (self.clips.iter().all(|clip| clip.contains(inside))).then_some(pixels)
    }

    pub fn copy_bytes(&self, pixels: IRect) -> Option<usize> {
        let inside = self.bounds.is_some_and(|bounds| bounds.contains(pixels));
        if !inside {
            return None;
        }
        pixel_bytes(pixels.size(), self.bytes_per_pixel)
    }
}

pub(super) fn fit(steps: &mut [Planned<'_>], budget: usize) {
    let mut held = hit_bytes(steps);
    drop_fills(steps, budget, &mut held);
}

fn hit_bytes(steps: &[Planned<'_>]) -> usize {
    (steps.iter())
        .map(|step| match step {
            Planned::Leaf(Leaf {
                cached: Cached::Hit { bytes, .. },
                ..
            }) => *bytes,
            Planned::Leaf(_) => 0,
            Planned::Group { steps, .. } | Planned::Graft(steps) => hit_bytes(steps),
        })
        .sum()
}

fn drop_fills(steps: &mut [Planned<'_>], budget: usize, held: &mut usize) {
    for step in steps {
        let leaf = match step {
            Planned::Leaf(leaf) => leaf,
            Planned::Group { steps, .. } | Planned::Graft(steps) => {
                drop_fills(steps, budget, held);
                continue;
            }
        };
        let Cached::Fill { bytes, .. } = leaf.cached else {
            continue;
        };
        match held.checked_add(bytes).filter(|total| *total <= budget) {
            Some(total) => *held = total,
            None => leaf.cached = Cached::No,
        }
    }
}

fn on_whole_pixels(rect: skia_safe::Rect) -> Option<IRect> {
    let pixels = rect.round();
    (skia_safe::Rect::from(pixels) == rect).then_some(pixels)
}

fn whole_pixels(matrix: &Matrix, rect: Rect) -> Option<IRect> {
    let (device, _rect_under_every_scale_translate) = matrix.map_rect(skrect(rect).ok()?);
    on_whole_pixels(device).filter(|pixels| !pixels.is_empty())
}
