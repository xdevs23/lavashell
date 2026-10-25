// SPDX-License-Identifier: GPL-3.0-only
use super::super::{PixelGeometry, TextDestination};
use super::tests::{new_painter, pixels, target};
use super::*;
use crate::test_error::TestError;
use crate::test_scenes::{paint, paint_damage};
use lavashell_core::scene::tree::SceneTree;
use lavashell_core::surface::SurfaceId;

#[test]
fn approximate_policy_refreshes_existing_retained_and_persistent_text_without_changing_metrics()
-> Result<(), TestError> {
    let mut painter = new_painter(PixelGeometry::None)?;
    let mut view = View::new(lavashell_test_platform::facts(), |ui| {
        ui.scoped("retained", Modifier::new(), |ui| {
            ui.text(
                "Headphones p e\u{301} office",
                Modifier::new()
                    .font_family("probe")
                    .font_size(13.sp())
                    .user_select(true)
                    .tag("text"),
            );
        });
    });
    let size = Size::new(300.dp(), 150.dp());
    view.set_accessibility_text_geometry(true);
    let viewport = Viewport::new(size, Density::ONE);
    let mut tree = SceneTree::default();
    tree.show(view.frame(viewport, &mut painter)?)?;
    view.update_accessibility();
    let initial_bounds = view.bounds_of("text");
    let first_text = view
        .accessibility()
        .nodes
        .values()
        .find_map(|node| node.text.as_ref())
        .ok_or(TestError::Missing("text node"))?
        .clone();
    let (mut persistent, info) = target()?;
    painter.effect_planner.surface = Some(SurfaceId(11));
    paint_damage(
        &mut painter,
        persistent.canvas(),
        &tree,
        Some(Rect::new(0.dp(), 0.dp(), 300.dp(), 150.dp())),
        skia_safe::Color::TRANSPARENT,
    )?;
    let gray = pixels(&mut persistent, &info);
    let compositions = view.stats().root_compositions;
    for geometry in [PixelGeometry::Rgb, PixelGeometry::Bgr, PixelGeometry::None] {
        assert!(!view.needs_frame());
        painter.set_text_rendering(
            TextRendering {
                antialiasing: TextAntialiasing::SubpixelApproximate,
                ..Default::default()
            },
            TextDestination {
                opaque: false,
                geometry,
            },
        );
        assert!(
            view.needs_frame(),
            "existing scopes ignored the new provider request"
        );
        tree.show(view.frame(viewport, &mut painter)?)?;
        view.update_accessibility();
        assert_eq!(view.bounds_of("text"), initial_bounds);
        assert_eq!(
            view.accessibility()
                .nodes
                .values()
                .find_map(|node| node.text.as_ref()),
            Some(&first_text)
        );
        assert_eq!(view.stats().root_compositions, compositions);
        painter.effect_planner.surface = Some(SurfaceId(11));
        paint_damage(
            &mut painter,
            persistent.canvas(),
            &tree,
            None,
            skia_safe::Color::TRANSPARENT,
        )?;
        let actual = pixels(&mut persistent, &info);
        let (mut fresh, _) = target()?;
        let _chained_canvas = fresh.canvas().clear(skia_safe::Color::TRANSPARENT);
        paint(
            &mut new_painter(geometry)?,
            fresh.canvas(),
            &tree,
            Density::ONE,
        );
        assert_eq!(
            actual,
            pixels(&mut fresh, &info),
            "empty damage retained stale pixels"
        );
        if geometry == PixelGeometry::None {
            assert_eq!(actual, gray);
        } else {
            assert_ne!(actual, gray);
        }
    }
    Ok(())
}
