// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::Painter;
use crate::failure::Failures;
use crate::raster::RasterCache;
use crate::test_error::TestError;
use crate::text::{Coverage, RasterInput, Rasterizers, Shaper, TextRasterizer};
use crate::text_cache::ShapedParagraph;
use crate::{
    BlendSpace,
    text::{Hinting, PixelGeometry, TextAntialiasing, TextDestination, TextRendering},
};
use lavashell_core::prelude::*;
use lavashell_core::protected_text::ProtectedText;
use lavashell_core::scene::tree::{
    Change, Content, Group, Node, Packet, SceneNodeId, SceneTree, SceneUpdate,
};
use lavashell_test_fixtures::WithoutFailures;
use skia_safe::{AlphaType, ColorSpace, ColorType, ImageInfo};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

fn new_painter() -> Result<Painter, TestError> {
    new_painter_with(&crate::test_font::test_font()?)
}

fn new_painter_with(fonts: &crate::Fonts) -> Result<Painter, TestError> {
    let mut painter = Painter::new(fonts, crate::UniformRange::VULKAN_REQUIRED)?
        .with_raster_mode(crate::raster::RasterMode::Tiles);
    painter.set_blend_space(BlendSpace::Srgb);
    Ok(painter)
}

fn find(scene: &SceneSnapshot) -> Option<PrivateText> {
    (scene.at(Duration::ZERO).leaf_commands()).find_map(|command| match command {
        DrawCommand::PrivateText { text, .. } => Some(text.clone()),
        _ => None,
    })
}

fn field(
    painter: &mut Painter,
    value: &str,
) -> Result<(View, SceneBuilder, ProtectedText, PrivateText), TestError> {
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
    let mut builder = SceneBuilder::default();
    (view.frame_into(viewport, &mut painter.shaper, &mut builder)).without_failures()?;
    view.input(InputEvent::Key {
        key: Key::Tab,
        shift: false,
    });
    view.input(InputEvent::Text(value.into()));
    secret.set_revealed(true);
    (view.frame_into(viewport, &mut painter.shaper, &mut builder)).without_failures()?;
    let tree = builder.snapshot();
    let private = find(&tree).ok_or(TestError::Missing("private text draw"))?;
    Ok((view, builder, secret, private))
}

fn pixels(painter: &mut Painter, commands: &[DrawCommand]) -> Result<Vec<u8>, TestError> {
    drawn(|canvas| painter.paint_as_leaf(canvas, commands, Density::ONE))
}

fn scene_pixels(painter: &mut Painter, scene: &SceneSnapshot) -> Result<Vec<u8>, TestError> {
    drawn(|canvas| {
        crate::test_scenes::paint(painter, canvas, scene, Density::ONE);
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
        painter.shaper.paragraphs().len(),
        painter.shaper.skia_cache_count(),
        painter.resources.tiles.borrow().images().count(),
        painter.images.len(),
        painter.packet_cache.stats.cache_bytes,
    )
}

#[test]
fn private_shaping_and_nested_paint_leave_reusable_caches_unchanged() -> Result<(), TestError> {
    let mut painter = new_painter()?;
    let (_view_not_framed_again, _builder_not_framed_again, secret, private) =
        field(&mut painter, "iWé💡e\u{301}אבג")?;
    let ordinary = painter.shaper.measure(
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
            node: Node::Leaf(Content::Packet(Packet::new(commands)?)),
        },
    ];
    let mut tree = SceneTree::default();
    tree.show(SceneUpdate { changes })?;
    let scene = tree.snapshot();
    for _ in 0..3 {
        let geometry = painter.shaper.private_geometry(&private, &style, 260.dp());
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
        let image = scene_pixels(&mut painter, &scene)?;
        assert!(image.as_chunks::<4>().0.iter().any(|pixel| pixel[3] > 0));
        let after = counts(&mut painter);
        assert!(after.0 <= before.0);
        assert_eq!(
            (after.1, after.2, after.3, after.4),
            (before.1, before.2, before.3, before.4)
        );
        assert_eq!(painter.shaper.private_skia_cache_count(), 0);
    }
    assert!(painter.packet_cache.stats.uncached_leaves >= 3);
    let debug = format!("{:?}", find(&scene));
    assert!(!debug.contains("iWé") && debug.contains("[redacted]"));
    Ok(())
}

#[test]
fn private_pixels_match_ordinary_font_and_coverage_policy_without_storing_tiles()
-> Result<(), TestError> {
    let mut painter = new_painter()?;
    let (_view_not_framed_again, _builder_not_framed_again, _secret_not_changed, private) =
        field(&mut painter, "Mon 5 Oct e\u{301} office אבג")?;
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
                assert_eq!(actual, expected);
                assert_eq!(painter.resources.tiles.borrow().images().count(), 0);
                assert_eq!(painter.shaper.private_skia_cache_count(), 0);
            }
        }
    }
    Ok(())
}

const ROUNDING_STEP_OF_TWO_SEPARATE_DRAWS: u8 = 1;

#[test]
fn a_weight_between_two_static_faces_adds_the_coverage_of_both_for_ordinary_and_private_text()
-> Result<(), TestError> {
    let fonts = crate::test_font::static_test_faces()?;
    let mut painter = new_painter_with(&fonts)?;
    let mut reference = new_painter_with(&fonts)?;
    let (_view_not_framed_again, _builder_not_framed_again, _secret_not_changed, private) =
        field(&mut painter, "Mon 5 Oct office")?;
    let rect = Rect::new(7.dp(), 2.dp(), 280.dp(), 40.dp());
    let style = |weight, alpha| -> Result<TextStyle<Dp>, TestError> {
        Ok(
            TextStyle::new("probe", 13.sp(), Color::WHITE.with_alpha(alpha))
                .weight(FontWeight::try_new(weight)?)
                .resolve_unscaled(),
        )
    };
    let ordinary = |style| {
        private.expose(|value| DrawCommand::Text {
            text: value.into(),
            rect,
            style,
        })
    };
    let alike = |a: &[u8], b: &[u8]| {
        (a.iter().zip(b)).all(|(a, b)| a.abs_diff(*b) <= ROUNDING_STEP_OF_TWO_SEPARATE_DRAWS)
    };
    let lighter_face = pixels(&mut reference, &[ordinary(style(400.0, 0.75)?)])?;
    let heavier_face = pixels(&mut reference, &[ordinary(style(700.0, 0.25)?)])?;
    let lighter_and_heavier_face: Vec<u8> = (lighter_face.iter().zip(&heavier_face))
        .map(|(lighter, heavier)| lighter.saturating_add(*heavier))
        .collect();
    let only_lighter_face = pixels(&mut reference, &[ordinary(style(400.0, 1.0)?)])?;
    assert!((lighter_and_heavier_face.as_chunks::<4>().0.iter()).any(|pixel| pixel[3] > 0));
    assert!(!alike(&lighter_and_heavier_face, &only_lighter_face));
    let between = style(475.0, 1.0)?;
    let protected = DrawCommand::PrivateText {
        text: private.clone(),
        rect,
        style: between.clone(),
    };
    for command in [ordinary(between), protected] {
        let actual = pixels(&mut painter, &[command])?;
        assert!(alike(&actual, &lighter_and_heavier_face));
    }
    Ok(())
}

#[test]
fn subpixel_text_stays_subpixel_from_one_static_face_through_the_blend_to_the_other()
-> Result<(), TestError> {
    let fonts = crate::test_font::static_test_faces()?;
    let mut painter = Painter::new(&fonts, crate::UniformRange::VULKAN_REQUIRED)?
        .with_raster_mode(crate::raster::RasterMode::Tiles);
    painter.set_text_rendering(
        TextRendering {
            antialiasing: TextAntialiasing::Subpixel,
            ..Default::default()
        },
        TextDestination {
            opaque: true,
            geometry: PixelGeometry::Rgb,
        },
    );
    let (_view_not_framed_again, _builder_not_framed_again, _secret_not_changed, private) =
        field(&mut painter, "Mon 5 Oct office")?;
    let rect = Rect::new(7.dp(), 2.dp(), 280.dp(), 40.dp());
    for weight in [400.0, 475.0, 700.0] {
        let style = TextStyle::new("probe", 13.sp(), Color::WHITE)
            .weight(FontWeight::try_new(weight)?)
            .resolve_unscaled();
        let ordinary = private.expose(|value| DrawCommand::Text {
            text: value.into(),
            rect,
            style: style.clone(),
        });
        let protected = DrawCommand::PrivateText {
            text: private.clone(),
            rect,
            style,
        };
        for command in [ordinary, protected] {
            let image =
                drawn_on_black(|canvas| painter.paint_as_leaf(canvas, &[command], Density::ONE))?;
            let colored_fringe = |pixel: &[u8; 4]| pixel[0] != pixel[1] || pixel[1] != pixel[2];
            assert!(
                image.as_chunks::<4>().0.iter().any(colored_fringe),
                "{weight}"
            );
        }
    }
    Ok(())
}

fn drawn_on_black(
    paint: impl FnOnce(&skia_safe::Canvas) -> Result<(), TestError>,
) -> Result<Vec<u8>, TestError> {
    let info = ImageInfo::new(
        (300, 48),
        ColorType::RGBA8888,
        AlphaType::Opaque,
        ColorSpace::new_srgb(),
    );
    let mut surface =
        skia_safe::surfaces::raster(&info, None, None).ok_or(TestError::RasterSurface)?;
    paint(surface.canvas().clear(skia_safe::Color::BLACK))?;
    let mut pixels = vec![0; 300 * 48 * 4];
    assert!(surface.read_pixels(&info, &mut pixels, 300 * 4, (0, 0)));
    Ok(pixels)
}

struct FixedOutcomeRasterizer(fn() -> Result<Option<Coverage>, crate::Error>);

impl TextRasterizer for FixedOutcomeRasterizer {
    fn antialiasing(&self) -> TextAntialiasing {
        TextAntialiasing::Grayscale
    }

    fn source_style(&self, input: &RasterInput<'_>) -> Option<TextStyle<Dp>> {
        Some(input.style.clone())
    }

    fn coverage(
        &self,
        _input_not_rasterized: &RasterInput<'_>,
        _origin_not_rasterized: skia_safe::Point,
        _source_not_rasterized: &mut ShapedParagraph,
        _key_not_rasterized: Option<u64>,
        _tiles_not_rasterized: &mut RasterCache,
    ) -> Result<Option<Coverage>, crate::Error> {
        (self.0)()
    }

    fn draw(
        &self,
        _input_not_drawn: &RasterInput<'_>,
        _coverage_never_produced: &Coverage,
    ) -> Result<(), crate::Error> {
        Ok(())
    }
}

#[test]
fn private_text_declined_or_failed_by_the_rasterizer_matches_ordinary_text_without_storing_tiles()
-> Result<(), TestError> {
    let grayscale = TextRendering {
        antialiasing: TextAntialiasing::Grayscale,
        ..Default::default()
    };
    let declined: fn() -> Result<Option<Coverage>, crate::Error> = || Ok(None);
    let failed: fn() -> Result<Option<Coverage>, crate::Error> = || Err(crate::Error::SubpixelTile);
    for (outcome, recorded_failures) in [(declined, 0), (failed, 1)] {
        let mut painter = new_painter()?;
        let (_view_not_framed_again, _builder_not_framed_again, _secret_not_changed, private) =
            field(&mut painter, "Mon 5 Oct office")?;
        painter.set_text_rendering(grayscale, TextDestination::default());
        painter.text_rasterizers = Rasterizers::only(Rc::new(FixedOutcomeRasterizer(outcome)));
        let mut reference = new_painter()?;
        reference.set_text_rendering(grayscale, TextDestination::default());
        let rect = Rect::new(7.dp(), 2.dp(), 280.dp(), 40.dp());
        let style = TextStyle::new("probe", 13.sp(), Color::WHITE).resolve_unscaled();
        let protected = DrawCommand::PrivateText {
            text: private.clone(),
            rect,
            style: style.clone(),
        };
        let ordinary = private.expose(|value| DrawCommand::Text {
            text: value.into(),
            rect,
            style,
        });
        let actual = pixels(&mut painter, &[protected])?;
        let expected = pixels(&mut reference, &[ordinary])?;
        assert!(expected.as_chunks::<4>().0.iter().any(|pixel| pixel[3] > 0));
        assert_eq!(actual, expected);
        assert_eq!(painter.resources.tiles.borrow().images().count(), 0);
        assert_eq!(painter.shaper.private_skia_cache_count(), 0);
        assert_eq!(
            painter.failures.count(DrawFailure::SubpixelText),
            recorded_failures
        );
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
    let (mut view, mut builder, secret, private) = field(&mut painter, "iWé office")?;
    let style = TextStyle::new("probe", 13.sp(), Color::WHITE).resolve_unscaled();
    let first = painter.shaper.private_geometry(&private, &style, 260.dp());
    let size = Size::new(300.dp(), 48.dp());
    let viewport = Viewport::new(size, Density::ONE);
    (view.frame_into(viewport, &mut painter.shaper, &mut builder)).without_failures()?;
    assert!(!view.needs_frame());
    painter.set_text_rendering(
        TextRendering {
            hinting: Hinting::Full,
            ..Default::default()
        },
        TextDestination::default(),
    );
    assert!(view.needs_frame());
    (view.frame_into(viewport, &mut painter.shaper, &mut builder)).without_failures()?;
    let second = painter.shaper.private_geometry(&private, &style, 260.dp());
    assert_ne!(carets(&first), carets(&second));
    assert!(!view.needs_frame());
    secret.set_revealed(false);
    secret.clear();
    assert!(view.needs_frame());
    (view.frame_into(viewport, &mut painter.shaper, &mut builder)).without_failures()?;
    assert!(find(&builder.snapshot()).is_none());
    assert!(
        view.accessibility()
            .nodes
            .values()
            .all(|node| node.value.is_none())
    );
    assert_eq!(painter.shaper.private_skia_cache_count(), 0);
    Ok(())
}

#[test]
fn long_private_runs_clip_transient_tiles_on_the_original_sampling_grid() -> Result<(), TestError> {
    let mut painter = new_painter()?;
    let (_view_not_framed_again, _builder_not_framed_again, _secret_not_changed, private) =
        field(&mut painter, &"W".repeat(4096))?;
    let style = TextStyle::new("probe", 13.sp(), Color::WHITE).resolve_unscaled();
    let mut paragraph = ShapedParagraph::new(private.expose(|value| {
        painter
            .shaper
            .private_paragraph(painter.text_rendering(), value, &style, 100_000.dp())
    })?);
    let mut surface =
        skia_safe::surfaces::raster_n32_premul((300, 48)).ok_or(TestError::RasterSurface)?;
    let canvas = surface.canvas().clip_rect(
        skia_safe::Rect::from_xywh(10.0, 3.0, 242.0, 35.0),
        None,
        false,
    );
    let origin = skia_safe::Point::new(-40_000.25, 2.25);
    let budget = crate::raster::TileBudget {
        bytes_per_pixel: 4,
        max_bytes: 32 * 1024 * 1024,
    };
    let original = crate::raster::ParagraphTile::new(canvas, paragraph.ink(), origin, budget)
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
        .paragraph_image(canvas, &mut paragraph, None, origin)?
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
    let mut reference = Shaper::new(crate::test_font::test_font()?.load()?, Failures::default());
    let style = TextStyle {
        max_lines: Some(1),
        ..TextStyle::new("probe", 13.sp(), Color::WHITE)
    }
    .resolve_unscaled();
    let ellipsis = reference.measure("…", &style, Dp::INFINITY).width;
    for value in ["i", "iWé💡 secret"] {
        let (_view_not_framed_again, _builder_not_framed_again, _secret_not_changed, private) =
            field(&mut painter, value)?;
        let width = reference.measure(value, &style, Dp::INFINITY).width;
        let before = counts(&mut painter);
        assert_eq!(
            painter.shaper.private_min_width(&private, &style),
            width.min(ellipsis)
        );
        assert_eq!(counts(&mut painter), before);
    }
    assert!(reference.measure("i", &style, Dp::INFINITY).width < ellipsis);
    assert_eq!(painter.shaper.private_skia_cache_count(), 0);
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
    let mut reference = Shaper::new(crate::test_font::test_font()?.load()?, Failures::default());
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
        let (_view_not_framed_again, _builder_not_framed_again, secret, private) =
            field(&mut painter, &value)?;
        let intrinsic = reference.measure(&value, &style, width);
        let masked = reference.measure(&secret.masked('•'), &style, width);
        let shaper = &mut painter.shaper;
        let before = shaper.paragraphs().len();
        assert_eq!(shaper.private_measure(&private, &style, width), intrinsic);
        assert_eq!(
            shaper.private_geometry(&private, &style, width).size.width,
            width
        );
        assert_eq!(shaper.paragraphs().len(), before);
        let mut view = field_beside_text(&secret);
        let mut builder = SceneBuilder::default();
        let viewport = Viewport::new(Size::new(width, 100.dp()), Density::ONE);
        for revealed in [true, false, true] {
            secret.set_revealed(revealed);
            (view.frame_into(viewport, &mut painter.shaper, &mut builder)).without_failures()?;
            let field = view
                .bounds_of(&"field")
                .ok_or(TestError::Missing("field node"))?;
            let sibling = view
                .bounds_of(&"sibling")
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
        assert_eq!(painter.shaper.private_skia_cache_count(), 0);
    }
    let (wide, width) = ("W".repeat(128), 600.dp());
    assert!(reference.measure(&wide, &style, Dp::INFINITY).width > width);
    let one_line = reference.measure("W", &style, width).height;
    let (_view_not_framed_again, _builder_not_framed_again, secret, _text_not_drawn) =
        field(&mut painter, &wide)?;
    let mut view = field_beside_text(&secret);
    let mut builder = SceneBuilder::default();
    let viewport = Viewport::new(Size::new(width, 100.dp()), Density::ONE);
    for revealed in [true, false] {
        secret.set_revealed(revealed);
        (view.frame_into(viewport, &mut painter.shaper, &mut builder)).without_failures()?;
        let field = view
            .bounds_of(&"field")
            .ok_or(TestError::Missing("field node"))?;
        assert_eq!(field.height, one_line);
    }
    Ok(())
}
