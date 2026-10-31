// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::test_error::TestError;
use crate::text_cache::ShapedParagraph;
use crate::{
    BlendSpace,
    text::{Hinting, PixelGeometry, TextAntialiasing, TextDestination, TextRendering},
};
use lavashell_core::protected_text::ProtectedText;
use lavashell_core::scene::tree::{
    Change, Content, Group, Node, Packet, SceneNodeId, SceneTree, SceneUpdate,
};
use lavashell_test_fixtures::WithoutFailures;
use skia_safe::{AlphaType, ColorSpace, ColorType, ImageInfo};
use std::sync::Arc;
use std::time::Duration;

fn new_painter() -> Result<Painter, TestError> {
    let fonts = crate::test_font::test_font()?;
    let mut painter =
        Painter::new(&fonts, crate::UniformRange::VULKAN_REQUIRED)?.with_raster_paths(true);
    painter.set_blend_space(BlendSpace::Srgb);
    Ok(painter)
}

fn find(tree: &SceneTree) -> Option<PrivateText> {
    (tree.at(Duration::ZERO).leaf_commands()).find_map(|command| match command {
        DrawCommand::PrivateText { text, .. } => Some(text.clone()),
        _ => None,
    })
}

fn field(
    painter: &mut Painter,
    value: &str,
) -> Result<(View, SceneTree, ProtectedText, PrivateText), TestError> {
    let secret = ProtectedText::new();
    let state = secret.clone();
    let mut view = View::new(lavashell_test_fixtures::facts(), move |ui| {
        let state = state.clone();
        ui.scoped("private field", Modifier::new(), move |ui| {
            ui.protected_input(
                state.clone(),
                TextFieldOptions::default(),
                Modifier::new()
                    .tag("private")
                    .font_family("probe")
                    .font_size(13.sp())
                    .size(260.dp(), 40.dp()),
            );
        });
    });
    let size = Size::new(300.dp(), 48.dp());
    let viewport = Viewport::new(size, Density::ONE);
    let mut tree = SceneTree::default();
    tree.show(view.frame(viewport, painter).without_failures()?)?;
    view.input(InputEvent::Key {
        key: Key::Tab,
        shift: false,
    });
    view.input(InputEvent::Text(value.into()));
    secret.set_revealed(true);
    tree.show(view.frame(viewport, painter).without_failures()?)?;
    let private = find(&tree).ok_or(TestError::Missing("private text draw"))?;
    Ok((view, tree, secret, private))
}

fn pixels(painter: &mut Painter, commands: &[DrawCommand]) -> Result<Vec<u8>, TestError> {
    drawn(|canvas| painter.paint_as_leaf(canvas, commands, Density::ONE))
}

fn tree_pixels(painter: &mut Painter, tree: &SceneTree) -> Result<Vec<u8>, TestError> {
    drawn(|canvas| {
        crate::test_scenes::paint(painter, canvas, tree, Density::ONE);
        Ok(())
    })
}

fn drawn(
    paint: impl FnOnce(&skia_safe::Canvas) -> Result<(), TestError>,
) -> Result<Vec<u8>, TestError> {
    let info = ImageInfo::new(
        (300, 48),
        ColorType::RGBA8888,
        AlphaType::Premul,
        ColorSpace::new_srgb(),
    );
    let mut surface =
        skia_safe::surfaces::raster(&info, None, None).ok_or(TestError::RasterSurface)?;
    paint(surface.canvas().clear(skia_safe::Color::TRANSPARENT))?;
    let mut pixels = vec![0; 300 * 48 * 4];
    assert!(surface.read_pixels(&info, &mut pixels, 300 * 4, (0, 0)));
    Ok(pixels)
}

fn carets(geometry: &TextGeometry) -> Vec<(usize, Rect)> {
    geometry
        .carets
        .iter()
        .map(|caret| (caret.index, caret.rect))
        .collect()
}

fn counts(painter: &mut Painter) -> (usize, i32, usize, usize, usize) {
    (
        painter.paragraphs.len(),
        painter.fonts.paragraph_cache_mut().count(),
        painter.resources.tiles.borrow().images().count(),
        painter.images.len(),
        painter.packet_cache.stats.cache_bytes,
    )
}

#[test]
fn private_shaping_and_nested_paint_leave_reusable_caches_unchanged() -> Result<(), TestError> {
    let mut painter = new_painter()?;
    let (_, _, secret, private) = field(&mut painter, "iWé💡e\u{301}אבג")?;
    let ordinary = painter.measure(
        "ordinary",
        &TextStyle::new("probe", 13.sp(), Color::WHITE).resolve_unscaled(),
        260.dp(),
    );
    assert!(ordinary.width > Dp::ZERO);
    let before = counts(&mut painter);
    assert!(before.0 > 0 && before.1 > 0);
    let bounds = Rect::new(0.dp(), 0.dp(), 300.dp(), 48.dp());
    let style = TextStyle::new("probe", 13.sp(), Color::WHITE).resolve_unscaled();
    let commands = Arc::from([DrawCommand::PrivateText {
        text: private.clone(),
        rect: bounds,
        style: style.clone(),
    }]);
    let changes = vec![
        Change::Insert {
            id: SceneNodeId(1),
            parent: None,
            index: 0,
            node: Node::Group(Group::default()),
        },
        Change::Insert {
            id: SceneNodeId(2),
            parent: Some(SceneNodeId(1)),
            index: 0,
            node: Node::Leaf(Content::Packet(Packet::new(commands))),
        },
    ];
    let mut tree = SceneTree::default();
    tree.show(SceneUpdate { changes })?;
    for _ in 0..3 {
        let geometry = painter.private_geometry(&private, &style, 260.dp());
        assert!(geometry.word_starts.is_none());
        assert!(
            geometry
                .carets
                .iter()
                .all(|caret| caret.index % "•".len() == 0)
        );
        assert_eq!(
            geometry.carets.last().map(|caret| caret.index),
            Some(secret.masked('•').len())
        );
        assert!(
            geometry
                .clusters
                .iter()
                .any(|c| c.direction == lavashell_core::accessibility::TextDirection::RightToLeft)
        );
        let image = tree_pixels(&mut painter, &tree)?;
        assert!(image.as_chunks::<4>().0.iter().any(|pixel| pixel[3] > 0));
        let after = counts(&mut painter);
        // Frames drop the paragraphs that they do not draw. Private text adds no paragraphs.
        assert!(after.0 <= before.0);
        assert_eq!(
            (after.1, after.2, after.3, after.4),
            (before.1, before.2, before.3, before.4)
        );
        assert_eq!(painter.private_text.fonts.paragraph_cache_mut().count(), 0);
    }
    assert!(painter.packet_cache.stats.uncached_leaves >= 3);
    let debug = format!("{:?}", find(&tree));
    assert!(!debug.contains("iWé") && debug.contains("[redacted]"));
    Ok(())
}

#[test]
fn private_pixels_match_ordinary_font_and_coverage_policy_without_storing_tiles()
-> Result<(), TestError> {
    let mut painter = new_painter()?;
    let (_, _, _, private) = field(&mut painter, "Mon 5 Oct e\u{301} office אבג")?;
    let mut reference = new_painter()?;
    for (antialiasing, geometry) in [
        (TextAntialiasing::Grayscale, PixelGeometry::None),
        (TextAntialiasing::SubpixelApproximate, PixelGeometry::Rgb),
        (TextAntialiasing::SubpixelApproximate, PixelGeometry::Bgr),
    ] {
        for hinting in [Hinting::None, Hinting::Slight, Hinting::Full] {
            let policy = TextRendering {
                hinting,
                antialiasing,
                ..Default::default()
            };
            let destination = TextDestination {
                opaque: false,
                geometry,
            };
            painter.set_text_rendering(policy, destination);
            reference.set_text_rendering(policy, destination);
            for phase in [0.0, 0.25, 0.75] {
                let rect = Rect::new(
                    Dp::new(7.0 + phase),
                    Dp::new(2.0 + phase),
                    280.dp(),
                    40.dp(),
                );
                let mut style = TextStyle::new("probe", 13.sp(), Color::WHITE).resolve_unscaled();
                style.color = Color::rgb(0x008c_b3df).with_alpha(0.55);
                let command = DrawCommand::PrivateText {
                    text: private.clone(),
                    rect,
                    style: style.clone(),
                };
                let ordinary = private.expose(|value| DrawCommand::Text {
                    text: value.into(),
                    rect,
                    style,
                });
                let actual = pixels(&mut painter, &[command])?;
                let expected = pixels(&mut reference, &[ordinary])?;
                assert!(actual.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 1));
                assert_eq!(painter.resources.tiles.borrow().images().count(), 0);
                assert_eq!(painter.private_text.fonts.paragraph_cache_mut().count(), 0);
            }
        }
    }
    Ok(())
}

#[test]
fn private_policy_changes_refresh_the_same_view_and_clear_reveal_without_a_draw()
-> Result<(), TestError> {
    let mut painter = new_painter()?;
    painter.set_text_rendering(
        TextRendering {
            hinting: Hinting::None,
            ..Default::default()
        },
        TextDestination::default(),
    );
    let (mut view, mut tree, secret, private) = field(&mut painter, "iWé office")?;
    let style = TextStyle::new("probe", 13.sp(), Color::WHITE).resolve_unscaled();
    let first = painter.private_geometry(&private, &style, 260.dp());
    let size = Size::new(300.dp(), 48.dp());
    let viewport = Viewport::new(size, Density::ONE);
    tree.show(view.frame(viewport, &mut painter).without_failures()?)?;
    assert!(!view.needs_frame());
    painter.set_text_rendering(
        TextRendering {
            hinting: Hinting::Full,
            ..Default::default()
        },
        TextDestination::default(),
    );
    assert!(view.needs_frame());
    tree.show(view.frame(viewport, &mut painter).without_failures()?)?;
    let second = painter.private_geometry(&private, &style, 260.dp());
    assert_ne!(carets(&first), carets(&second));
    assert!(!view.needs_frame());
    secret.set_revealed(false);
    secret.clear();
    assert!(view.needs_frame());
    tree.show(view.frame(viewport, &mut painter).without_failures()?)?;
    assert!(find(&tree).is_none());
    assert!(
        view.accessibility()
            .nodes
            .values()
            .all(|node| node.value.is_none())
    );
    assert_eq!(painter.private_text.fonts.paragraph_cache_mut().count(), 0);
    Ok(())
}

#[test]
fn long_private_runs_clip_transient_tiles_on_the_original_sampling_grid() -> Result<(), TestError> {
    let mut painter = new_painter()?;
    let (_, _, _, private) = field(&mut painter, &"W".repeat(4096))?;
    let style = TextStyle::new("probe", 13.sp(), Color::WHITE).resolve_unscaled();
    let mut paragraph = ShapedParagraph::new(private.expose(|value| {
        painter.private_text.paragraph(
            &painter.registry,
            painter.text_rendering(),
            value,
            &style,
            100_000.dp(),
        )
    })?);
    let mut surface =
        skia_safe::surfaces::raster_n32_premul((300, 48)).ok_or(TestError::RasterSurface)?;
    let canvas = surface.canvas().clip_rect(
        skia_safe::Rect::from_xywh(10.0, 3.0, 242.0, 35.0),
        None,
        false,
    );
    let original = crate::raster::ParagraphTile::new(
        canvas,
        paragraph.ink(),
        (-40_000.25, 2.25),
        4,
        32 * 1024 * 1024,
    )
    .ok_or(TestError::Missing("paragraph tile"))?;
    assert!(original.size.width > 8192);
    let clip = canvas
        .local_clip_bounds()
        .ok_or(TestError::Missing("clip bounds"))?;
    let clipped = original
        .clipped(clip, 4)
        .ok_or(TestError::Missing("clipped tile"))?;
    assert!(clipped.size.width <= 244 && clipped.size.height <= 37);
    assert!(clipped.bytes <= 244 * 37 * 4);
    let (image, bounds) = painter
        .resources
        .tiles
        .get_mut()
        .without_storage(|tiles| {
            tiles.paragraph_image(canvas, &mut paragraph, 0, (-40_000.25, 2.25))
        })
        .ok_or(TestError::Missing("paragraph image"))?;
    assert_eq!(image.dimensions(), clipped.size);
    assert_eq!(bounds, clipped.destination);
    assert_eq!(painter.resources.tiles.borrow().images().count(), 0);
    Ok(())
}

#[test]
fn the_smallest_private_width_is_shaped_from_the_value_outside_the_shared_caches()
-> Result<(), TestError> {
    let mut painter = new_painter()?;
    let mut reference = new_painter()?;
    let style = TextStyle {
        max_lines: Some(1),
        ..TextStyle::new("probe", 13.sp(), Color::WHITE)
    }
    .resolve_unscaled();
    let ellipsis = reference.measure("…", &style, Dp::INFINITY).width;
    for value in ["i", "iWé💡 secret"] {
        let (_, _, _, private) = field(&mut painter, value)?;
        let width = reference.measure(value, &style, Dp::INFINITY).width;
        let before = counts(&mut painter);
        assert_eq!(
            painter.private_min_width(&private, &style),
            width.min(ellipsis)
        );
        assert_eq!(counts(&mut painter), before);
    }
    assert!(reference.measure("i", &style, Dp::INFINITY).width < ellipsis);
    assert_eq!(painter.private_text.fonts.paragraph_cache_mut().count(), 0);
    Ok(())
}

fn field_beside_text(secret: &ProtectedText) -> View {
    let state = secret.clone();
    View::new(lavashell_test_fixtures::facts(), move |ui| {
        ui.layout(Axis::Horizontal, Modifier::new(), |ui| {
            ui.protected_input(
                state.clone(),
                TextFieldOptions::default(),
                Modifier::new()
                    .font_family("probe")
                    .font_size(13.sp())
                    .tag("field"),
            );
            ui.text(
                "tail",
                Modifier::new()
                    .font_family("probe")
                    .font_size(13.sp())
                    .tag("sibling"),
            );
        });
    })
}

#[test]
fn private_intrinsic_measurement_does_not_expand_a_row_or_displace_its_sibling()
-> Result<(), TestError> {
    let mut painter = new_painter()?;
    let mut reference = new_painter()?;
    let style = TextStyle {
        max_lines: Some(1),
        ..TextStyle::new("probe", 13.sp(), Color::WHITE)
    }
    .resolve_unscaled();
    for (value, width) in [
        ("iWi".to_owned(), 600.dp()),
        ("iWé💡e\u{301}אבג".to_owned(), 600.dp()),
        ("W".repeat(128), 4096.dp()),
    ] {
        let (_, _, secret, private) = field(&mut painter, &value)?;
        let intrinsic = reference.measure(&value, &style, width);
        let masked = reference.measure(&secret.masked('•'), &style, width);
        let before = painter.paragraphs.len();
        assert_eq!(painter.private_measure(&private, &style, width), intrinsic);
        assert_eq!(
            painter.private_geometry(&private, &style, width).size.width,
            width
        );
        assert_eq!(painter.paragraphs.len(), before);
        let mut view = field_beside_text(&secret);
        let mut tree = SceneTree::default();
        let viewport = Viewport::new(Size::new(width, 100.dp()), Density::ONE);
        for revealed in [true, false, true] {
            secret.set_revealed(revealed);
            (view.show_frame(&mut tree, viewport, &mut painter)).without_failures()?;
            let field = view
                .bounds_of("field")
                .ok_or(TestError::Missing("field node"))?;
            let sibling = view
                .bounds_of("sibling")
                .ok_or(TestError::Missing("sibling node"))?;
            let expected = if revealed { intrinsic } else { masked };
            assert_eq!(field.size(), expected);
            assert!(
                field.width < width,
                "geometry constraint became intrinsic field width"
            );
            assert!(sibling.x >= field.x + field.width);
            assert!(
                sibling.x + sibling.width <= width,
                "intrinsic field displaced its sibling outside the row"
            );
        }
        assert_eq!(painter.private_text.fonts.paragraph_cache_mut().count(), 0);
    }
    let (wide, width) = ("W".repeat(128), 600.dp());
    assert!(reference.measure(&wide, &style, Dp::INFINITY).width > width);
    let one_line = reference.measure("W", &style, width).height;
    let (_, _, secret, _) = field(&mut painter, &wide)?;
    let mut view = field_beside_text(&secret);
    let mut tree = SceneTree::default();
    let viewport = Viewport::new(Size::new(width, 100.dp()), Density::ONE);
    for revealed in [true, false] {
        secret.set_revealed(revealed);
        (view.show_frame(&mut tree, viewport, &mut painter)).without_failures()?;
        let field = view
            .bounds_of("field")
            .ok_or(TestError::Missing("field node"))?;
        assert_eq!(field.height, one_line);
    }
    Ok(())
}
