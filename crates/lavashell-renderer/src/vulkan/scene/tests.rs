// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::test_error::TestError;
use crate::{Error, Fonts, Painter, SurfaceFrame};
use lavashell_core::prelude::{Density, Dp, Rect};
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
            density: Density::ONE,
            damage: Some(Rect::new(Dp::ZERO, Dp::ZERO, side, side)),
            clear: skia_safe::Color::TRANSPARENT,
            scene: Some(scene),
            shape: None,
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
