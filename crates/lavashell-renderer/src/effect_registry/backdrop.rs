// SPDX-License-Identifier: GPL-3.0-only
use super::{
    DrawTarget, EffectBackend, EffectRegistration, EffectRegistry, EffectRequest, SurfaceAccess,
};
use crate::failure::FailedDraw;
use crate::field::{FieldRoom, Solid};
use crate::raster::{pixel_bytes, resource_cache_limit};
use crate::resources::{ImageCache, PaintResources};
use glass::{BlurLevel, Clip, GlassPrograms, capture_bounds};
use lavashell_core::prelude::*;
use lavashell_core::surface::SurfaceId;
use lavashell_effect_quality::{DrawId, EffectId, OfferedVariant, VariantId, VariantQuality};
use lru::LruCache;
use skia_safe::{Canvas, Image, Paint};
use std::{cell::RefCell, iter, rc::Rc};

const ID: EffectId = EffectId(0x6261_636b_6472_6f70);

const TINT: VariantId = VariantId(0);

const CACHED: VariantId = VariantId(1);

const FULL_BLUR: VariantId = VariantId(2);

inventory::submit! { EffectRegistration { install } }

fn install(registry: &mut EffectRegistry, room: &FieldRoom) -> Result<(), crate::Error> {
    let backend = Backend {
        shared: Rc::new(RefCell::new(Shared::new(GlassPrograms::new(room)?))),
    };
    registry.register_effect(ID, |c| matches!(c, DrawCommand::Backdrop { .. }), backend)
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Mode {
    Blur(u8),
    Cached,
    Tint,
}

impl Mode {
    fn of(variant: VariantId) -> Option<Self> {
        match variant {
            TINT => Some(Self::Tint),
            CACHED => Some(Self::Cached),
            VariantId(id) => {
                let level = id.checked_sub(FULL_BLUR.0)?;
                u8::try_from(level).ok().map(Self::Blur)
            }
        }
    }

    fn variant(self) -> VariantId {
        match self {
            Self::Tint => TINT,
            Self::Cached => CACHED,
            Self::Blur(level) => VariantId(FULL_BLUR.0 + u64::from(level)),
        }
    }

    fn quality(self) -> VariantQuality {
        match self {
            Self::Tint => VariantQuality(0),
            Self::Cached => VariantQuality(1),
            Self::Blur(level) => VariantQuality(2 + u32::from(u8::MAX - level)),
        }
    }

    fn offered(self) -> OfferedVariant {
        OfferedVariant {
            variant: self.variant(),
            quality: self.quality(),
        }
    }
}

fn blur_levels(effect: &Backdrop, pixels_per_dp: f64) -> impl Iterator<Item = Mode> {
    let effect = *effect;
    let spans_a_pixel = move |level: &u8| {
        let pixels = BlurLevel::new(&effect, *level).effect.blur.get().get() * pixels_per_dp;
        pixels.is_finite() && pixels >= 1.0
    };
    iter::once(0)
        .chain((1..=u8::MAX).take_while(spans_a_pixel))
        .map(Mode::Blur)
}

fn matrix_bits(matrix: &skia_safe::Matrix) -> [u32; 9] {
    let mut values = [0.0; 9];
    matrix.get_9(&mut values);
    values.map(f32::to_bits)
}

struct Cached {
    surface: SurfaceId,
    image: Image,
    optics: Shape,
    effect: Backdrop,
    matrix: [u32; 9],
    bytes: usize,
    device_bounds: skia_safe::IRect,
    scene_generation: u64,
}

struct Shared {
    glass: GlassPrograms,
    entries: LruCache<DrawId, Cached>,
    bytes: usize,
}

impl Shared {
    fn new(glass: GlassPrograms) -> Self {
        Self {
            glass,
            entries: LruCache::unbounded(),
            bytes: 0,
        }
    }

    fn store(&mut self, id: DrawId, cached: Cached, limit: usize) -> &Cached {
        if let Some(older) = self.entries.pop(&id) {
            self.bytes -= older.bytes;
        }
        while self.bytes.saturating_add(cached.bytes) > limit
            && let Some((_, evicted)) = self.entries.pop_lru()
        {
            self.bytes -= evicted.bytes;
        }
        self.bytes += cached.bytes;
        self.entries.get_or_insert(id, || cached)
    }

    fn forget_surface(&mut self, surface: SurfaceId) {
        self.entries.retain(|_, entry| entry.surface != surface);
        self.bytes = self.entries.iter().map(|(_, entry)| entry.bytes).sum();
    }
}

struct Backend {
    shared: Rc<RefCell<Shared>>,
}

impl EffectBackend for Backend {
    fn variants(&self, command: &DrawCommand, target: DrawTarget) -> Vec<OfferedVariant> {
        let DrawCommand::Backdrop { effect, .. } = command else {
            return Vec::new();
        };
        (blur_levels(effect, target.pixels_per_dp))
            .chain([Mode::Cached, Mode::Tint])
            .map(Mode::offered)
            .collect()
    }

    fn reads_destination(&self) -> bool {
        true
    }

    fn uses_command_footprint(&self) -> bool {
        true
    }

    fn render(&mut self, canvas: &Canvas, request: &EffectRequest<'_>) {
        let resources = request.painter.resources();
        (resources.held).set(ImageCache::Backdrops, self.shared.borrow().bytes);
        if let Err(failure) = self.draw(canvas, request) {
            request.painter.record(failure);
        }
    }

    fn forget_surface(&mut self, surface: SurfaceId) {
        self.shared.borrow_mut().forget_surface(surface);
    }
}

impl Backend {
    fn draw(&mut self, canvas: &Canvas, request: &EffectRequest<'_>) -> Result<(), FailedDraw> {
        let DrawCommand::Backdrop { shape, effect } = request.command else {
            return Ok(());
        };
        match Mode::of(request.variant) {
            Some(Mode::Tint) | None => Ok(()),
            Some(Mode::Cached) => self.draw_cached(canvas, request, shape, effect),
            Some(Mode::Blur(level)) => self.draw_blur(canvas, request, shape, effect, level),
        }
    }

    fn draw_cached(
        &self,
        canvas: &Canvas,
        request: &EffectRequest<'_>,
        shape: &Shape,
        effect: &Backdrop,
    ) -> Result<(), FailedDraw> {
        if request.access != SurfaceAccess::Cacheable {
            return Ok(());
        }
        let matrix = canvas.local_to_device_as_3x3();
        let Some(expected) = capture_bounds(canvas, matrix, shape.bounds(), 0.0)? else {
            return Ok(());
        };
        let optics = Solid::of(shape);
        let mut shared = self.shared.borrow_mut();
        let cached = shared.entries.get(&request.id).filter(|entry| {
            entry.surface == request.surface
                && entry.optics == *optics
                && entry.effect == *effect
                && entry.matrix == matrix_bits(&matrix)
                && entry.device_bounds == expected
                && (request.input)
                    .is_some_and(|input| input.scene_generation == entry.scene_generation)
        });
        match cached {
            Some(cached) => composite(canvas, request.painter.resources(), shape, cached),
            None => Ok(()),
        }
    }

    fn draw_blur(
        &self,
        canvas: &Canvas,
        request: &EffectRequest<'_>,
        shape: &Shape,
        effect: &Backdrop,
        level: u8,
    ) -> Result<(), FailedDraw> {
        let reduced = BlurLevel::new(effect, level);
        let mut shared = self.shared.borrow_mut();
        let resources = request.painter.resources();
        let direct = |glass: &GlassPrograms| {
            glass.backdrop(canvas, resources, shape, reduced, request.access)
        };
        let matrix = canvas.local_to_device_as_3x3();
        let input = request
            .input
            .filter(|_| request.access == SurfaceAccess::Cacheable);
        let Some(input) = input.filter(|_| glass::axis_aligned(&matrix)) else {
            return direct(&shared.glass);
        };
        let Some(bounds) = capture_bounds(canvas, matrix, shape.bounds(), 0.0)? else {
            return Ok(());
        };
        let limit = resources.held.budget(ImageCache::Backdrops, canvas);
        let bytes = pixel_bytes(bounds.size(), canvas.image_info().bytes_per_pixel())
            .filter(|bytes| *bytes <= limit);
        let Some(bytes) = bytes else {
            return direct(&shared.glass);
        };
        let optics = Solid::of(shape);
        let image = raw_snapshot(
            canvas,
            &shared.glass,
            resources,
            matrix,
            &optics,
            reduced,
            bounds,
        )?;
        let Some(image) = image else {
            return direct(&shared.glass);
        };
        let cached = Cached {
            surface: request.surface,
            image,
            optics: optics.into_shape(),
            effect: *effect,
            matrix: matrix_bits(&matrix),
            bytes,
            device_bounds: bounds,
            scene_generation: input.scene_generation,
        };
        let cached = shared.store(request.id, cached, limit);
        let composited = composite(canvas, resources, shape, cached);
        (resources.held).set(ImageCache::Backdrops, shared.bytes);
        composited
    }
}

fn raw_snapshot(
    canvas: &Canvas,
    glass: &GlassPrograms,
    resources: &PaintResources,
    matrix: skia_safe::Matrix,
    optics: &Solid,
    level: BlurLevel,
    bounds: skia_safe::IRect,
) -> Result<Option<Image>, FailedDraw> {
    let size = canvas.base_layer_size();
    let bytes = pixel_bytes(size, canvas.image_info().bytes_per_pixel());
    if bytes.is_none_or(|bytes| bytes > resource_cache_limit(canvas)) {
        return Ok(None);
    }
    let Some(background) = canvas.snapshot_surface(None) else {
        return Ok(None);
    };
    let info = canvas.image_info().with_dimensions(size);
    let Some(mut raw) = canvas.new_surface(&info, None) else {
        return Ok(None);
    };
    let prepared = (raw.canvas())
        .clear(skia_safe::Color::TRANSPARENT)
        .draw_image(&background, (0.0, 0.0), None)
        .clip_rect(skia_safe::Rect::from(bounds), None, false)
        .set_matrix(&skia_safe::M44::from(matrix));
    glass.apply(
        prepared,
        resources,
        optics,
        level,
        Clip::Bounds,
        SurfaceAccess::Current,
    )?;
    Ok(raw.image_snapshot_with_bounds(bounds))
}

fn composite(
    canvas: &Canvas,
    resources: &PaintResources,
    shape: &Shape,
    cached: &Cached,
) -> Result<(), FailedDraw> {
    glass::within(canvas, resources, Clip::Shape(shape), || {
        let _chained_canvas = canvas.reset_matrix().draw_image_rect(
            &cached.image,
            None,
            skia_safe::Rect::from(cached.device_bounds),
            Paint::default().set_blend_mode(skia_safe::BlendMode::Src),
        );
    })
}

mod glass;
#[cfg(test)]
mod hole_tests;
#[cfg(test)]
mod lens_tests;
#[cfg(test)]
mod tests;
