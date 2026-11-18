// SPDX-License-Identifier: GPL-3.0-only
use crate::test_error::TestError;
use crate::test_scenes::{TEST_SURFACE, leaves};
use crate::test_surfaces::TestSurface;
use crate::{Painter, StreamFrames, SurfaceFrame};
use lavashell_core::prelude::*;
use lavashell_core::retained::UnbalancedRestore;
use lavashell_core::scene::tree::Packet;
use std::sync::Arc;
use std::time::Duration;

pub const SURFACE: TestSurface = TestSurface::new(240, 120);

pub fn packet(
    rect: Rect,
    paint: impl FnOnce(&mut Drawing<'_>),
) -> Result<Packet, UnbalancedRestore> {
    let mut commands = Vec::new();
    let context = DrawContext::new(Viewport::new(rect.size(), Density::ONE));
    paint(&mut Drawing::new(&mut commands, rect, &context));
    Packet::new(Arc::from(commands))
}

pub fn repainted_pixels_on(
    target: &mut skia_safe::Surface,
    painter: &mut Painter,
    shape: Option<&skia_safe::Path>,
    frames: impl IntoIterator<Item = (SceneSnapshot, Rect)>,
) -> Result<Vec<u64>, TestError> {
    (frames.into_iter())
        .map(|(tree, damage)| -> Result<u64, TestError> {
            let before = painter.render_stats().damaged_pixels;
            let frame = SurfaceFrame {
                surface: TEST_SURFACE,
                tree: tree.at(Duration::ZERO),
                dp_to_output: Affine::IDENTITY,
                damage: Some(damage),
                clear: skia_safe::Color::TRANSPARENT,
                scene: None,
                shape,
                streams: &StreamFrames::default(),
            };
            painter.paint_surface(target.canvas(), frame)?;
            Ok(painter.render_stats().damaged_pixels - before)
        })
        .collect()
}

pub fn damage_frames(
    damages: [Rect; 4],
    make_packets: impl Fn() -> Result<Vec<Packet>, UnbalancedRestore>,
) -> Result<Vec<(SceneSnapshot, Rect)>, TestError> {
    let mut frames = Vec::new();
    for damage in damages {
        frames.push((leaves(make_packets()?)?, damage));
    }
    Ok(frames)
}

pub const DAMAGE_CUTTING_LEAVES: Rect =
    Rect::new(Dp::new(30.0), Dp::new(10.0), Dp::new(80.0), Dp::new(10.0));

#[must_use]
pub fn full_damage_then_cuts() -> [Rect; 4] {
    [
        SURFACE.bounds(),
        DAMAGE_CUTTING_LEAVES,
        DAMAGE_CUTTING_LEAVES,
        DAMAGE_CUTTING_LEAVES,
    ]
}

pub fn card(rect: Rect) -> Result<Packet, UnbalancedRestore> {
    packet(SURFACE.bounds(), |d| d.fill(rect, Color::rgb(0x002a_4d8f)))
}
