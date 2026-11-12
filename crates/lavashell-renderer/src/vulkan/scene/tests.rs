// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::test_error::TestError;
use crate::test_surfaces::pixels;
use crate::{Error, Fonts, Painter, StreamFrames, SurfaceFrame};
use lavashell_core::prelude::{Density, Dp, DpExt, PixelScale, PixelSize, Rect, Size, Viewport};
use lavashell_core::scene::tree::SceneTree;

const SIDE: u16 = 8;

fn size() -> (u32, u32) {
    (u32::from(SIDE), u32::from(SIDE))
}

fn paint(painter: &mut Painter, canvas: &Canvas, scene: &SceneImage) -> Result<(), Error> {
    let tree = SceneTree::default();
    let side = Dp::new(f64::from(SIDE));
    painter.paint_surface(
        canvas,
        SurfaceFrame {
            tree: tree.at(Duration::ZERO),
            dp_to_output: PixelScale::SQUARE.dp_to_output(Density::ONE),
            damage: Some(Rect::new(Dp::ZERO, Dp::ZERO, side, side)),
            clear: skia_safe::Color::TRANSPARENT,
            scene: Some(scene),
            shape: None,
            streams: &StreamFrames::default(),
        },
    )
}

#[test]
fn a_scene_image_is_painted_only_on_a_canvas_of_the_renderer_that_imported_it()
-> Result<(), TestError> {
    let mut importer = VulkanRenderer::headless()?;
    let mut other = VulkanRenderer::headless()?;
    let mut painter = importer.painter(&Fonts::new())?;
    let mut produced = importer.offscreen(size())?;
    let pending = importer.submit_scene(&mut produced, 1, Duration::ZERO)?;
    let scene = &pending.scene.content;

    let mut target = importer.offscreen(size())?;
    paint(&mut painter, target.canvas(), scene)?;

    let mut target = other.offscreen(size())?;
    let foreign = paint(&mut painter, target.canvas(), scene);
    assert!(matches!(
        foreign,
        Err(Error::SceneOfAnotherRenderer {
            canvas_owner: Some(_),
            ..
        })
    ));

    let mut raster = crate::test_surfaces::surface(i32::from(SIDE), i32::from(SIDE))?;
    let without_gpu = paint(&mut painter, raster.canvas(), scene);
    assert!(matches!(
        without_gpu,
        Err(Error::SceneOfAnotherRenderer {
            canvas_owner: None,
            ..
        })
    ));

    drop(pending);
    Ok(())
}

#[test]
fn a_scene_image_fills_a_target_drawn_at_a_wide_scale() -> Result<(), TestError> {
    let viewport = Viewport::new(Size::new(40.dp(), 30.dp()), Density::ONE);
    let scale = PixelScale::filling(viewport, PixelSize::new(64, 30))?;
    let size = scale.buffer_size(viewport)?;
    let mut gpu = VulkanRenderer::headless()?;
    let mut painter = gpu.painter(&Fonts::new())?;
    let mut produced = gpu.offscreen((size.width, size.height))?;
    let _chained_canvas = produced.canvas().clear(skia_safe::Color::BLUE);
    let pending = gpu.submit_scene(&mut produced, 1, Duration::ZERO)?;
    let mut target = gpu.offscreen((size.width, size.height))?;
    let tree = SceneTree::default();
    let frame = SurfaceFrame {
        tree: tree.at(Duration::ZERO),
        dp_to_output: scale.dp_to_output(viewport.density),
        damage: Some(Rect::from_size(viewport.size)),
        clear: skia_safe::Color::TRANSPARENT,
        scene: Some(&pending.scene.content),
        shape: None,
        streams: &StreamFrames::default(),
    };
    painter.paint_surface(target.canvas(), frame)?;
    let shown = pixels(&mut target);
    let blue = [0, 0, 255, 255];
    assert!(shown.as_chunks::<4>().0.iter().all(|pixel| *pixel == blue));
    drop(pending);
    Ok(())
}
