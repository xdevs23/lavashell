// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::test_error::TestError;
use crate::test_scenes::TEST_SURFACE;
use crate::test_surfaces::TestSurface;
use crate::{Error, Fonts, Painter, StreamFrames, SurfaceFrame};
use lavashell_core::prelude::Density;
use lavashell_core::scene::tree::SceneTree;

const SURFACE: TestSurface = TestSurface::new(8, 8);

fn paint(painter: &mut Painter, canvas: &Canvas, scene: &SceneImage) -> Result<(), Error> {
    let tree = SceneTree::default();
    painter.paint_surface(
        canvas,
        SurfaceFrame {
            surface: TEST_SURFACE,
            tree: tree.at(Duration::ZERO),
            density: Density::ONE,
            damage: Some(SURFACE.bounds()),
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
    let mut produced = importer.offscreen(SURFACE.pixel_size())?;
    let pending = importer.submit_scene(&mut produced, 1, Duration::ZERO)?;
    let scene = &pending.scene.content;

    let mut target = importer.offscreen(SURFACE.pixel_size())?;
    paint(&mut painter, target.canvas(), scene)?;

    let mut target = other.offscreen(SURFACE.pixel_size())?;
    let foreign = paint(&mut painter, target.canvas(), scene);
    assert!(matches!(
        foreign,
        Err(Error::SceneOfAnotherRenderer {
            canvas_owner: Some(_),
            ..
        })
    ));

    let mut raster = SURFACE.raster()?;
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
