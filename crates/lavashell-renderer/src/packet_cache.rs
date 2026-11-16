// SPDX-License-Identifier: GPL-3.0-only
use crate::plan::{Cached, Leaf, Placement};
use crate::repeated_leaves::RepeatedLeafFilter;
use crate::resources::{HeldImages, ImageCache};
use crate::{Painter, StreamFrames};
use lavashell_core::frame::RenderStats;
use lavashell_core::prelude::*;
use lavashell_core::scene::tree::{NodeKey, Packet, PacketGeneration};
use lavashell_core::surface::SurfaceId;
use lru::LruCache;
use skia_safe::{BlendMode, Canvas, IPoint, IRect, Image, Matrix, Paint};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PacketCaching {
    Enabled,
    Disabled,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct PacketCacheKey {
    pub surface: SurfaceId,
    pub leaf: NodeKey,
    generation: PacketGeneration,
    device: [u32; 4],
}

impl PacketCacheKey {
    pub fn new(surface: SurfaceId, leaf: NodeKey, packet: &Packet, matrix: &Matrix) -> Self {
        let device = [
            matrix.scale_x(),
            matrix.scale_y(),
            matrix.translate_x(),
            matrix.translate_y(),
        ];
        Self {
            surface,
            leaf,
            generation: packet.generation(),
            device: device.map(f32::to_bits),
        }
    }
}

struct CachedPacket {
    image: Image,
    bytes: usize,
    pass: u64,
}

pub(crate) struct PacketCache {
    repeated: RepeatedLeafFilter,
    entries: LruCache<PacketCacheKey, CachedPacket>,
    caching: PacketCaching,
    pass: u64,
    pub stats: RenderStats,
}

impl PacketCache {
    pub fn new() -> Self {
        Self {
            repeated: RepeatedLeafFilter::default(),
            entries: LruCache::unbounded(),
            caching: PacketCaching::Enabled,
            pass: 0,
            stats: RenderStats::default(),
        }
    }

    pub fn begin(&mut self) -> u64 {
        self.pass += 1;
        self.pass
    }

    pub fn retain_shown(&mut self, surface: SurfaceId, shown: &HashSet<NodeKey>) {
        self.repeated.retain_shown(surface, shown);
    }

    pub fn cached(
        &mut self,
        surface: SurfaceId,
        leaf: NodeKey,
        packet: &Packet,
        cover: Rect,
        placement: &Placement<'_>,
    ) -> Cached {
        let Some(pixels) = placement.pixels(cover) else {
            return Cached::No;
        };
        let key = PacketCacheKey::new(surface, leaf, packet, placement.matrix);
        match self.entries.get_mut(&key) {
            Some(entry) => {
                entry.pass = self.pass;
                Cached::Hit {
                    image: entry.image.clone(),
                    pixels,
                    bytes: entry.bytes,
                }
            }
            None => (placement.copy_bytes(pixels))
                .map_or(Cached::No, |bytes| self.repeated_fill(key, pixels, bytes)),
        }
    }

    fn repeated_fill(&mut self, key: PacketCacheKey, pixels: IRect, bytes: usize) -> Cached {
        if self.repeated.repeats(key) {
            Cached::Fill { key, pixels, bytes }
        } else {
            Cached::No
        }
    }

    pub fn budget(&self, canvas: &Canvas, held: &HeldImages) -> usize {
        match self.caching {
            PacketCaching::Enabled => held.budget(ImageCache::Packets, canvas),
            PacketCaching::Disabled => 0,
        }
    }

    pub fn set_caching(&mut self, caching: PacketCaching, held: &HeldImages) {
        self.caching = caching;
        if caching == PacketCaching::Disabled {
            self.clear(held);
        }
    }

    pub fn clear(&mut self, held: &HeldImages) {
        self.remove_where(|_| true, held);
    }

    pub fn forget_surface(&mut self, surface: SurfaceId, held: &HeldImages) {
        self.repeated.forget_surface(surface);
        self.remove_where(|key| key.surface == surface, held);
    }

    fn remove_where(&mut self, remove: impl Fn(&PacketCacheKey) -> bool, held: &HeldImages) {
        let removed: Vec<PacketCacheKey> = (self.entries.iter())
            .map(|(key, _)| *key)
            .filter(|key| remove(key))
            .collect();
        for key in removed {
            if let Some(entry) = self.entries.pop(&key) {
                self.stats.cache_bytes -= entry.bytes;
            }
        }
        self.report_held(held);
    }

    fn trim(&mut self, reserve: usize, kept: u64, held: &HeldImages, canvas: &Canvas) {
        let limit = self.budget(canvas, held);
        while self.stats.cache_bytes.saturating_add(reserve) > limit
            && self
                .entries
                .peek_lru()
                .is_some_and(|(_, oldest)| oldest.pass < kept)
        {
            if let Some((_, evicted)) = self.entries.pop_lru() {
                self.stats.cache_bytes -= evicted.bytes;
            }
        }
        self.report_held(held);
    }

    fn make_room(
        &mut self,
        canvas: &Canvas,
        key: PacketCacheKey,
        bytes: usize,
        kept: u64,
        held: &HeldImages,
    ) {
        let replaced = |old: &PacketCacheKey| {
            old.surface == key.surface && old.leaf == key.leaf && *old != key
        };
        self.remove_where(replaced, held);
        self.trim(bytes, kept, held, canvas);
    }

    fn insert(&mut self, key: PacketCacheKey, image: Image, bytes: usize, held: &HeldImages) {
        self.stats.cache_misses += 1;
        self.stats.cache_bytes += bytes;
        let pass = self.pass;
        if let Some((_, replaced)) = self.entries.push(key, CachedPacket { image, bytes, pass }) {
            self.stats.cache_bytes -= replaced.bytes;
        }
        self.report_held(held);
    }

    fn report_held(&self, held: &HeldImages) {
        held.set(ImageCache::Packets, self.stats.cache_bytes);
    }
}

impl Painter {
    pub fn render_stats(&self) -> RenderStats {
        self.packet_cache.stats
    }

    pub fn clear_packet_cache(&mut self) {
        self.packet_cache.clear(&self.resources.held);
    }

    pub fn set_packet_caching(&mut self, caching: PacketCaching) {
        self.packet_cache.set_caching(caching, &self.resources.held);
    }

    pub(crate) fn paint_leaf(
        &mut self,
        canvas: &Canvas,
        leaf: &Leaf<'_>,
        extent: IRect,
        streams: &StreamFrames,
        pass: u64,
    ) {
        let (key, pixels, bytes) = match &leaf.cached {
            Cached::Hit { image, pixels, .. } => {
                self.packet_cache.stats.cache_hits += 1;
                composite(canvas, image, *pixels);
                return;
            }
            Cached::No => {
                self.packet_cache.stats.uncached_leaves += 1;
                self.paint_commands(canvas, leaf, extent, streams);
                return;
            }
            Cached::Fill { key, pixels, bytes } => (*key, *pixels, *bytes),
        };
        self.paint_commands(canvas, leaf, extent, streams);
        (self.packet_cache).make_room(canvas, key, bytes, pass, &self.resources.held);
        match canvas.snapshot_surface(Some(pixels)) {
            Some(image) => (self.packet_cache).insert(key, image, bytes, &self.resources.held),
            None => self.packet_cache.stats.uncached_leaves += 1,
        }
    }
}

fn composite(canvas: &Canvas, image: &Image, pixels: IRect) {
    let mut copy = Paint::default();
    let _chained_paint = copy.set_blend_mode(BlendMode::Src);
    let saved = canvas.save();
    let _chained_canvas = canvas
        .reset_matrix()
        .draw_image(image, IPoint::new(pixels.left, pixels.top), Some(&copy))
        .restore_to_count(saved);
}
