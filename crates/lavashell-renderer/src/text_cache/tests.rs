// SPDX-License-Identifier: GPL-3.0-only
use super::TextCache;
use crate::float_asserts::assert_f32_eq;
use crate::scalar::scalar;
use crate::test_error::TestError;
use crate::test_scenes::TEST_SURFACE;
use crate::{Fonts, Painter};
use lavashell_core::prelude::*;
use lavashell_core::scene::tree::SceneTree;
use lavashell_core::surface::SurfaceId;
use skia_safe::{AlphaType, ColorSpace, ColorType, ImageInfo};
use std::time::Duration;

impl TextCache {
    pub(crate) fn len(&self) -> usize {
        self.paragraphs.values().count()
    }

    pub(crate) fn existing(
        &mut self,
        key: u64,
    ) -> Result<&mut skia_safe::textlayout::Paragraph, TestError> {
        let cached = (self.paragraphs).get_or_try_make(key, &self.frames, |_| {
            Err(TestError::Missing("paragraph under its key"))
        })?;
        Ok(&mut cached.shaped.paragraph)
    }
}

fn paint(
    painter: &mut Painter,
    surface: SurfaceId,
    commands: &[DrawCommand],
) -> Result<(), TestError> {
    let info = ImageInfo::new(
        (200, 40),
        ColorType::RGBA8888,
        AlphaType::Premul,
        ColorSpace::new_srgb(),
    );
    let mut target =
        skia_safe::surfaces::raster(&info, None, None).ok_or(TestError::RasterSurface)?;
    let tree = SceneTree::with_leaf(commands)?;
    painter.paint_scene(
        target.canvas(),
        surface,
        tree.at(Duration::ZERO),
        Density::ONE,
    );
    Ok(())
}

#[test]
fn text_measured_after_a_paint_leaves_its_paragraphs_in_the_cache() -> Result<(), TestError> {
    let mut painter = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
    let style = TextStyle::new("sans-serif", 14.sp(), Color::WHITE).resolve_unscaled();
    let rect = Rect::new(0.dp(), 0.dp(), 200.dp(), 40.dp());
    let label_key = painter
        .shaper
        .paragraph_input("Brightness", &style, rect.width, painter.text_rendering())?
        .key();
    let label = DrawCommand::Text {
        text: "Brightness".into(),
        rect,
        style: style.clone(),
    };
    paint(&mut painter, TEST_SURFACE, std::slice::from_ref(&label))?;
    let shaper = &mut painter.shaper;
    let one_line = shaper.measure("Brightness", &style, Dp::INFINITY).width;
    for width in 0_u16..600 {
        let width = Dp::new(300.0 + f64::from(width));
        assert_eq!(shaper.measure("Brightness", &style, width).width, one_line);
    }
    let cached_label = shaper.paragraphs().existing(label_key)?;
    assert_f32_eq(cached_label.max_width(), scalar(rect.width.get())?);

    let grown = shaper.paragraphs().len();
    paint(&mut painter, TEST_SURFACE, &[])?;
    paint(&mut painter, TEST_SURFACE, &[])?;
    assert!(painter.shaper.measure("Contrast", &style, rect.width).width < rect.width);
    assert!(painter.shaper.paragraphs().len() < grown);
    Ok(())
}

#[test]
fn text_measured_after_three_surfaces_paint_keeps_the_first_paragraph() -> Result<(), TestError> {
    let mut painter = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
    let style = TextStyle::new("sans-serif", 14.sp(), Color::WHITE).resolve_unscaled();
    let rect = Rect::new(0.dp(), 0.dp(), 200.dp(), 40.dp());
    let label_key = painter
        .shaper
        .paragraph_input("Brightness", &style, rect.width, painter.text_rendering())?
        .key();
    let label = DrawCommand::Text {
        text: "Brightness".into(),
        rect,
        style: style.clone(),
    };
    for surface in [1, 2, 3] {
        let commands = if surface == 1 {
            std::slice::from_ref(&label)
        } else {
            &[]
        };
        paint(&mut painter, SurfaceId::numbered(surface), commands)?;
    }
    let shaper = &mut painter.shaper;
    let one_line = shaper.measure("Contrast", &style, Dp::INFINITY).width;
    for width in 0_u16..600 {
        let width = Dp::new(300.0 + f64::from(width));
        assert_eq!(shaper.measure("Contrast", &style, width).width, one_line);
    }
    let cached_label = shaper.paragraphs().existing(label_key)?;
    assert_f32_eq(cached_label.max_width(), scalar(rect.width.get())?);
    Ok(())
}
