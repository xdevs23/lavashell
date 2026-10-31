// SPDX-License-Identifier: GPL-3.0-only
use crate::destination::Destination;
use crate::plan::{Area, Cached, Leaf, Placement};
use crate::repeated_leaves::RepeatedLeafFilter;
use crate::resources::{HeldImages, ImageCache};
use crate::scalar::scalars;
use crate::vulkan::SceneImage;
use crate::{Error, Painter, StreamFrames};
use lavashell_core::frame::RenderStats;
use lavashell_core::prelude::*;
use lavashell_core::scene::tree::{NodeKey, Packet, PacketGeneration, Presented};
use lavashell_core::surface::SurfaceId;
use lru::LruCache;
use skia_safe::{BlendMode, Canvas, IPoint, IRect, Image, Matrix, Paint, RoundOut};
use std::collections::HashSet;
use std::rc::Rc;

/// Whether the painter stores pixel copies of packets that repeat between plans. The copies count against the share
/// of the resource cache limit of the canvas that is given to packet copies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PacketCaching {
    Enabled,
    Disabled,
}

#[derive(Clone, Copy)]
pub struct SurfaceFrame<'a> {
    pub tree: Presented<'a>,
    pub density: Density,
    pub damage: Option<Rect>,
    pub clear: skia_safe::Color,
    pub scene: Option<&'a SceneImage>,
    /// An outline in physical pixels that clips the drawing, applied after the damaged pixels are cleared.
    pub shape: Option<&'a skia_safe::Path>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct PacketCacheKey {
    pub surface: SurfaceId,
    pub leaf: NodeKey,
    generation: PacketGeneration,
    /// The bits of the scales and the translations that map the packet into device pixels.
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
    /// The last pass that used or filled the copy.
    pass: u64,
}

/// Pixel copies of packets. When a new copy does not fit, the least recently used copies are evicted first.
pub(crate) struct PacketCache {
    repeated: RepeatedLeafFilter,
    entries: LruCache<PacketCacheKey, CachedPacket>,
    caching: PacketCaching,
    pass: u64,
    held: Rc<HeldImages>,
    pub stats: RenderStats,
}

impl PacketCache {
    /// A cache that reports the bytes of its copies to `held`. The other image caches report to the same `held`.
    pub fn new(held: Rc<HeldImages>) -> Self {
        Self {
            repeated: RepeatedLeafFilter::default(),
            entries: LruCache::unbounded(),
            caching: PacketCaching::Enabled,
            pass: 0,
            held,
            stats: RenderStats::default(),
        }
    }

    /// Starts a pass, and returns its number.
    pub fn begin(&mut self) -> u64 {
        self.pass += 1;
        self.pass
    }

    pub fn retain_shown(&mut self, surface: SurfaceId, shown: &HashSet<NodeKey>) {
        self.repeated.retain_shown(surface, shown);
    }

    /// The stored copy to draw in place of `leaf`. Without a stored copy, returns a copy to fill when `leaf` has the
    /// same key as in its previous plan.
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

    /// A copy of `pixels` to fill under `key` when the leaf has the same key as in its previous plan.
    fn repeated_fill(&mut self, key: PacketCacheKey, pixels: IRect, bytes: usize) -> Cached {
        if self.repeated.repeats(key) {
            Cached::Fill { key, pixels, bytes }
        } else {
            Cached::No
        }
    }

    /// The most bytes that the copies may use on `canvas`, next to the images of the other image caches.
    pub fn budget(&self, canvas: &Canvas) -> usize {
        match self.caching {
            PacketCaching::Enabled => self.held.budget(ImageCache::Packets, canvas),
            PacketCaching::Disabled => 0,
        }
    }

    pub fn set_caching(&mut self, caching: PacketCaching) {
        self.caching = caching;
        if caching == PacketCaching::Disabled {
            self.clear();
        }
    }

    pub fn clear(&mut self) {
        self.remove_where(|_| true);
    }

    pub fn forget_surface(&mut self, surface: SurfaceId) {
        self.repeated.forget_surface(surface);
        self.remove_where(|key| key.surface == surface);
    }

    fn remove_where(&mut self, remove: impl Fn(&PacketCacheKey) -> bool) {
        let removed: Vec<PacketCacheKey> = (self.entries.iter())
            .map(|(key, _)| *key)
            .filter(|key| remove(key))
            .collect();
        for key in removed {
            if let Some(entry) = self.entries.pop(&key) {
                self.stats.cache_bytes -= entry.bytes;
            }
        }
        self.report_held();
    }

    /// Evicts the least recently used copies that were last used before pass `kept`, until `reserve` more bytes fit
    /// in `limit`.
    fn trim(&mut self, reserve: usize, kept: u64, limit: usize) {
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
        self.report_held();
    }

    /// Removes every other copy of the leaf of `key`. Then evicts copies last used before pass `kept` until `bytes`
    /// more fit on `canvas`.
    fn make_room(&mut self, canvas: &Canvas, key: PacketCacheKey, bytes: usize, kept: u64) {
        self.remove_where(|old| old.surface == key.surface && old.leaf == key.leaf && *old != key);
        self.trim(bytes, kept, self.budget(canvas));
    }

    fn insert(&mut self, key: PacketCacheKey, image: Image, bytes: usize) {
        self.stats.cache_misses += 1;
        self.stats.cache_bytes += bytes;
        let pass = self.pass;
        if let Some((_, replaced)) = self.entries.push(key, CachedPacket { image, bytes, pass }) {
            self.stats.cache_bytes -= replaced.bytes;
        }
        self.report_held();
    }

    fn report_held(&self) {
        self.held.set(ImageCache::Packets, self.stats.cache_bytes);
    }
}

impl Painter {
    pub fn set_frame_sequence(&mut self, frame: u64) {
        self.frame = frame;
    }

    pub fn render_stats(&self) -> RenderStats {
        self.packet_cache.stats
    }

    pub fn clear_packet_cache(&mut self) {
        self.packet_cache.clear();
    }

    /// Releases the Skia images and GPU resources of the painter while the GPU context of the host still exists. The
    /// call also drops the effect implementations, so only the teardown of the host calls it.
    pub fn retire_gpu_resources(&mut self) {
        self.paint_configuration.clear();
        self.clear_packet_cache();
        self.images.clear();
        self.resources.retire();
        self.effect_planner.retire_gpu_resources();
        self.effect_registry = crate::effect_registry::EffectRegistry::empty();
        // The backdrop images belonged to the backdrop effect, which was dropped with the registry.
        self.resources.held.set(ImageCache::Backdrops, 0);
    }

    pub fn set_packet_caching(&mut self, caching: PacketCaching) {
        self.packet_cache.set_caching(caching);
    }

    /// Repaints only the damaged pixels of a surface whose pixels persist from frame to frame. A swapchain image
    /// needs a full composite, because each image of a swapchain has different earlier contents.
    ///
    /// # Errors
    /// Returns an error before painting when the scene image was imported by another renderer than the one
    /// that owns `canvas`.
    pub fn paint_surface(&mut self, canvas: &Canvas, frame: SurfaceFrame<'_>) -> Result<(), Error> {
        let SurfaceFrame {
            tree,
            density,
            damage,
            clear,
            scene,
            shape,
        } = frame;
        let scene = scene.map(|scene| scene.layer_of(canvas)).transpose()?;
        let surface = self.effect_planner.current_surface();
        let target = IRect::from_size(canvas.base_layer_size());
        let pixels = |length: Dp| density.to_px(length).get();
        let damage = damage.map(|damage| {
            let lengths = [damage.x, damage.y, damage.width, damage.height].map(pixels);
            scalars(lengths).map_or(target, |[x, y, width, height]| {
                skia_safe::Rect::from_xywh(x, y, width, height).round_out()
            })
        });
        // After a change of the paint configuration, the packets can be unchanged and report no damage while their
        // pixels change. Each persistent target is therefore repainted in full once.
        let damage = if self.paint_configuration.needs_reconstruction(surface) {
            Some(target)
        } else {
            damage
        };
        let Some(damage) = damage else {
            return Ok(());
        };
        let plan = self.plan(canvas, tree, density, shape);
        // A reader effect can sample outside the footprint of the packet. A layer around the whole tree has the size
        // of the repainted area. In both cases the whole target is repainted.
        let whole = plan.unknown_sampling || self.compositing_destination() == Destination::Layer;
        let repainted = plan.repaint(if whole { target } else { damage });
        let saved = canvas.save();
        let _chained_canvas = canvas.clip_irect(repainted, None).clear(clear);
        if let Some(shape) = shape {
            let _chained_canvas = canvas.clip_path(shape, None, true);
        }
        if let Some(scene) = &scene {
            scene.draw();
        }
        self.packet_cache.stats.damaged_pixels +=
            (repainted.width_64() * repainted.height_64()).cast_unsigned();
        let streams = StreamFrames::default();
        self.paint_tree(canvas, &plan, Area::Pixels(repainted), density, &streams);
        self.paint_configuration.painted(surface);
        let _chained_canvas = canvas.restore_to_count(saved);
        Ok(())
    }

    /// `pass` is the number of the pass that planned `leaf`.
    pub(crate) fn paint_leaf(
        &mut self,
        canvas: &Canvas,
        leaf: &Leaf<'_>,
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
                self.paint_commands(canvas, leaf, streams);
                return;
            }
            Cached::Fill { key, pixels, bytes } => (*key, *pixels, *bytes),
        };
        self.paint_commands(canvas, leaf, streams);
        self.packet_cache.make_room(canvas, key, bytes, pass);
        match canvas.snapshot_surface(Some(pixels)) {
            Some(image) => self.packet_cache.insert(key, image, bytes),
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
