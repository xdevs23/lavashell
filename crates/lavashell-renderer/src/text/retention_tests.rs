// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::test_error::TestError;
use crate::test_font::probe_modifier;
use crate::test_scenes::{commands, paint, paint_damage};
use crate::test_surfaces::{TestSurface, pixels};
use crate::text::sample_fixtures::{ACCENTED_LABEL_TEXT, PLAIN_LABEL_TEXT};
use lavashell_core::prelude::*;
use lavashell_core::surface::SurfaceId;
use lavashell_test_fixtures::WithoutFailures;

const RETENTION_SURFACE: TestSurface = TestSurface::new(400, 80);

fn bounds_of(view: &View, tag: &'static str) -> Result<Rect, TestError> {
    view.bounds_of(&tag).ok_or(TestError::Missing(tag))
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
                .filter(|text| node.bounds == bounds && text.value == ACCENTED_LABEL_TEXT)
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
    let mut view = View::new(lavashell_test_fixtures::facts(), |ui| {
        ui.layout(Axis::Vertical, Modifier::new(), |ui| {
            ui.text(
                ACCENTED_LABEL_TEXT,
                probe_modifier().user_select(true).tag("root text"),
            );
            ui.scoped("retained text", Modifier::new(), |ui| {
                ui.text(
                    ACCENTED_LABEL_TEXT,
                    probe_modifier().user_select(true).tag("component text"),
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
    let mut builder = SceneBuilder::default();
    (view.frame_into(viewport, &mut painter.shaper, &mut builder)).without_failures()?;
    view.update_accessibility();
    let first = [
        snapshot(&view, "root text")?,
        snapshot(&view, "component text")?,
    ];
    let compositions = view.stats().root_compositions;
    (view.frame_into(viewport, &mut painter.shaper, &mut builder)).without_failures()?;
    view.update_accessibility();
    assert!(!view.needs_frame());
    assert!(view.stats().reused_components > 0);
    painter.set_text_rendering(hinted, target);
    assert!(
        view.needs_frame(),
        "renderer policy did not notify the existing Core scopes"
    );
    (view.frame_into(viewport, &mut painter.shaper, &mut builder)).without_failures()?;
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
    (view.frame_into(viewport, &mut painter.shaper, &mut builder)).without_failures()?;
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
    let mut view = View::new(lavashell_test_fixtures::facts(), |ui| {
        ui.scoped("static label", Modifier::new(), |ui| {
            ui.text(PLAIN_LABEL_TEXT, probe_modifier().tag("static"));
        });
    });
    let target = crate::test_surfaces::lcd_destination();
    let size = Size::new(600.dp(), 100.dp());
    painter.set_text_rendering(TextRendering::default(), target);
    let viewport = Viewport::new(size, Density::ONE);
    let mut builder = SceneBuilder::default();
    (view.frame_into(viewport, &mut painter.shaper, &mut builder)).without_failures()?;
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
    (view.frame_into(viewport, &mut painter.shaper, &mut builder)).without_failures()?;
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
    let target = crate::test_surfaces::lcd_destination();
    let size = RETENTION_SURFACE.size();
    let mut view = View::new(lavashell_test_fixtures::facts(), move |ui| {
        ui.canvas(size, Modifier::new(), move |drawing: &mut Drawing<'_>| {
            drawing.fill(Rect::from_size(size), Color::rgb(0x0018_3e6c));
        });
        ui.text(PLAIN_LABEL_TEXT, probe_modifier());
    });
    let info = skia_safe::ImageInfo::new(
        RETENTION_SURFACE.dimensions(),
        skia_safe::ColorType::BGRA8888,
        skia_safe::AlphaType::Opaque,
        None,
    );
    let make = || {
        skia_safe::surfaces::raster(&info, None, Some(&target.surface_props()))
            .ok_or(TestError::RasterSurface)
    };
    let mut surfaces = [make()?, make()?];
    painter.set_text_rendering(TextRendering::default(), target);
    let viewport = Viewport::new(size, Density::ONE);
    let mut builder = SceneBuilder::default();
    (view.frame_into(viewport, &mut painter.shaper, &mut builder)).without_failures()?;
    let tree = builder.snapshot();
    let first_commands = commands(&tree);
    for (id, surface) in (1_u64..).zip(&mut surfaces) {
        paint_damage(
            &mut painter,
            SurfaceId::numbered(id),
            surface.canvas(),
            &tree,
            Affine::IDENTITY,
            Some(Rect::from_size(size)),
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
    (view.frame_into(viewport, &mut painter.shaper, &mut builder)).without_failures()?;
    let tree = builder.snapshot();
    assert_eq!(
        commands(&tree),
        first_commands,
        "this fixture must exercise invisible portable-command changes"
    );
    let mut reference = make()?;
    paint(&mut painter, reference.canvas(), &tree, Density::ONE)?;
    let expected = pixels(&mut reference);
    assert_ne!(
        first, expected,
        "the actual AA pixel coverage did not change"
    );
    for (id, surface) in (1_u64..).zip(&mut surfaces) {
        paint_damage(
            &mut painter,
            SurfaceId::numbered(id),
            surface.canvas(),
            &tree,
            Affine::IDENTITY,
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
