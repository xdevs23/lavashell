// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::test_error::TestError;
use crate::test_scenes::{commands, paint, paint_damage};
use lavashell_core::prelude::*;
use lavashell_core::scene::tree::SceneTree;
use lavashell_core::surface::SurfaceId;

fn bounds_of(view: &View, tag: &'static str) -> Result<Rect, TestError> {
    view.bounds_of(tag).ok_or(TestError::Missing(tag))
}

fn snapshot(view: &View, tag: &'static str) -> Result<(Rect, Vec<Rect>), TestError> {
    let bounds = bounds_of(view, tag)?;
    let text = view
        .accessibility()
        .nodes
        .values()
        .find_map(|node| {
            node.text
                .as_ref()
                .filter(|text| node.bounds == bounds && text.value == "Mon 5 Oct e\u{301} office")
        })
        .ok_or(TestError::Missing("text node"))?;
    let glyphs = text
        .runs
        .iter()
        .flat_map(|run| run.character_bounds.iter().copied())
        .collect();
    Ok((bounds, glyphs))
}

#[test]
fn policy_changes_invalidate_the_same_view_and_its_retained_component() -> Result<(), TestError> {
    let fonts = crate::test_font::test_font()?;
    let mut painter = crate::Painter::new(&fonts, crate::UniformRange::VULKAN_REQUIRED)?;
    let mut view = View::new(lavashell_test_platform::facts(), |ui| {
        ui.layout(Axis::Vertical, Modifier::new(), |ui| {
            ui.text(
                "Mon 5 Oct e\u{301} office",
                Modifier::new()
                    .font_family("probe")
                    .font_size(13.sp())
                    .user_select(true)
                    .tag("root text"),
            );
            ui.scoped("retained text", Modifier::new(), |ui| {
                ui.text(
                    "Mon 5 Oct e\u{301} office",
                    Modifier::new()
                        .font_family("probe")
                        .font_size(13.sp())
                        .user_select(true)
                        .tag("component text"),
                );
            });
        });
    });
    let target = TextDestination::default();
    let unhinted = TextRendering {
        hinting: Hinting::None,
        ..Default::default()
    };
    let hinted = TextRendering {
        hinting: Hinting::Full,
        ..Default::default()
    };
    let size = Size::new(1200.dp(), 200.dp());
    view.set_accessibility_text_geometry(true);
    painter.set_text_rendering(unhinted, target);
    let viewport = Viewport::new(size, Density::ONE);
    let mut tree = SceneTree::default();
    view.show_frame(&mut tree, viewport, &mut painter)?;
    view.update_accessibility();
    let first = [
        snapshot(&view, "root text")?,
        snapshot(&view, "component text")?,
    ];
    let compositions = view.stats().root_compositions;
    view.show_frame(&mut tree, viewport, &mut painter)?;
    view.update_accessibility();
    assert!(!view.needs_frame());
    assert!(view.stats().reused_components > 0);
    painter.set_text_rendering(hinted, target);
    assert!(
        view.needs_frame(),
        "renderer policy did not notify the existing Core scopes"
    );
    view.show_frame(&mut tree, viewport, &mut painter)?;
    view.update_accessibility();
    let other = [
        snapshot(&view, "root text")?,
        snapshot(&view, "component text")?,
    ];
    for (old, new) in first.iter().zip(&other) {
        assert_ne!(
            old.0.width, new.0.width,
            "retained measured width ignored hinting"
        );
        assert_ne!(
            old.1, new.1,
            "retained selectable caret geometry ignored hinting"
        );
    }
    assert_eq!(view.stats().root_compositions, compositions);
    assert!(!view.needs_frame());
    painter.set_text_rendering(unhinted, target);
    assert!(view.needs_frame());
    view.show_frame(&mut tree, viewport, &mut painter)?;
    view.update_accessibility();
    assert_eq!(
        [
            snapshot(&view, "root text")?,
            snapshot(&view, "component text")?
        ],
        first
    );
    assert_eq!(view.stats().root_compositions, compositions);
    assert!(!view.needs_frame());
    painter.set_text_rendering(unhinted, target);
    assert!(
        !view.needs_frame(),
        "unchanged policy caused an unnecessary frame"
    );
    Ok(())
}

#[test]
fn antialiasing_changes_refresh_static_packets_when_measured_bounds_are_identical()
-> Result<(), TestError> {
    let fonts = crate::test_font::test_font()?;
    let mut painter = crate::Painter::new(&fonts, crate::UniformRange::VULKAN_REQUIRED)?;
    let mut view = View::new(lavashell_test_platform::facts(), |ui| {
        ui.scoped("static label", Modifier::new(), |ui| {
            ui.text(
                "Mon 5 Oct office",
                Modifier::new()
                    .font_family("probe")
                    .font_size(13.sp())
                    .tag("static"),
            );
        });
    });
    let target = TextDestination {
        opaque: true,
        geometry: PixelGeometry::Rgb,
    };
    let size = Size::new(600.dp(), 100.dp());
    painter.set_text_rendering(TextRendering::default(), target);
    let viewport = Viewport::new(size, Density::ONE);
    let mut tree = SceneTree::default();
    view.show_frame(&mut tree, viewport, &mut painter)?;
    view.update_accessibility();
    let first_bounds = bounds_of(&view, "static")?;
    let painted_before = view.stats().painted_components;
    assert!(!view.needs_frame());
    painter.set_text_rendering(
        TextRendering {
            antialiasing: TextAntialiasing::Subpixel,
            ..Default::default()
        },
        target,
    );
    assert!(view.needs_frame());
    view.show_frame(&mut tree, viewport, &mut painter)?;
    view.update_accessibility();
    assert_eq!(bounds_of(&view, "static")?, first_bounds);
    assert!(
        view.stats().painted_components > painted_before,
        "unchanged metrics left a stale painting"
    );
    assert!(!view.needs_frame());
    Ok(())
}

#[test]
fn antialiasing_reconstructs_each_persistent_target_even_with_empty_command_damage()
-> Result<(), TestError> {
    let fonts = crate::test_font::test_font()?;
    let mut painter = crate::Painter::new(&fonts, crate::UniformRange::VULKAN_REQUIRED)?;
    let target = TextDestination {
        opaque: true,
        geometry: PixelGeometry::Rgb,
    };
    let size = Size::new(400.dp(), 80.dp());
    let mut view = View::new(lavashell_test_platform::facts(), |ui| {
        ui.canvas(
            Size::new(400.dp(), 80.dp()),
            Modifier::new(),
            |drawing: &mut Drawing<'_>| {
                drawing.fill(
                    Rect::new(0.dp(), 0.dp(), 400.dp(), 80.dp()),
                    Color::rgb(0x0018_3e6c),
                );
            },
        );
        ui.text(
            "Mon 5 Oct office",
            Modifier::new().font_family("probe").font_size(13.sp()),
        );
    });
    let info = skia_safe::ImageInfo::new(
        (400, 80),
        skia_safe::ColorType::BGRA8888,
        skia_safe::AlphaType::Opaque,
        None,
    );
    let make = || {
        skia_safe::surfaces::raster(&info, None, Some(&target.surface_props()))
            .ok_or(TestError::RasterSurface)
    };
    let pixels = |surface: &mut skia_safe::Surface| {
        let mut data = vec![0_u8; 400 * 80 * 4];
        assert!(surface.read_pixels(&info, &mut data, 400 * 4, (0, 0)));
        data
    };
    let mut surfaces = [make()?, make()?];
    painter.set_text_rendering(TextRendering::default(), target);
    let viewport = Viewport::new(size, Density::ONE);
    let mut tree = SceneTree::default();
    tree.show(view.frame(viewport, &mut painter)?)?;
    let first_commands = commands(&tree);
    for (index, surface) in surfaces.iter_mut().enumerate() {
        painter.effect_planner.surface = Some(SurfaceId(index as u64 + 1));
        paint_damage(
            &mut painter,
            surface.canvas(),
            &tree,
            Some(Rect::new(0.dp(), 0.dp(), size.width, size.height)),
            skia_safe::Color::TRANSPARENT,
        )?;
    }
    let first = pixels(&mut surfaces[0]);
    painter.set_text_rendering(
        TextRendering {
            antialiasing: TextAntialiasing::Subpixel,
            ..Default::default()
        },
        target,
    );
    tree.show(view.frame(viewport, &mut painter)?)?;
    assert_eq!(
        commands(&tree),
        first_commands,
        "this fixture must exercise invisible portable-command changes"
    );
    let mut reference = make()?;
    paint(&mut painter, reference.canvas(), &tree, Density::ONE);
    let expected = pixels(&mut reference);
    assert_ne!(
        first, expected,
        "the actual AA pixel coverage did not change"
    );
    for (index, surface) in surfaces.iter_mut().enumerate() {
        painter.effect_planner.surface = Some(SurfaceId(index as u64 + 1));
        paint_damage(
            &mut painter,
            surface.canvas(),
            &tree,
            None,
            skia_safe::Color::TRANSPARENT,
        )?;
        assert_eq!(
            pixels(surface),
            expected,
            "persistent target remained stale after policy change"
        );
    }
    Ok(())
}
