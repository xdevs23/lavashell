// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::test_error::TestError;
use lavashell_core::prelude::{
    Brush, Color, Density, DpExt, DrawCommand, PixelSize, SceneSnapshot, SceneTree, Size,
};

fn viewport() -> Viewport {
    Viewport::new(Size::new(40.dp(), 30.dp()), Density::ONE)
}

fn whole() -> Rect {
    Rect::from_size(viewport().size)
}

fn scene() -> Result<SceneSnapshot, TestError> {
    let fill = DrawCommand::Fill {
        shape: Rect::new(0.dp(), 0.dp(), 10.dp(), 10.dp()).into(),
        brush: Brush::Solid(Color::WHITE),
    };
    Ok(SceneTree::with_leaf([fill])?.snapshot())
}

fn inputs(lineage: TreeId) -> PixelInputs {
    PixelInputs {
        lineage,
        scene_generation: 0,
        scale: PixelScale::SQUARE,
        quality: QualityChange::Unchanged,
    }
}

type ReadContents = fn() -> Result<SurfaceContents, TestError>;

fn current() -> ReadContents {
    || Ok(SurfaceContents::Current)
}

fn outdated() -> ReadContents {
    || Ok(SurfaceContents::Outdated)
}

fn unreadable() -> Result<SurfaceContents, TestError> {
    Err(TestError::Unexpected(
        "a repaint without a read of the target",
    ))
}

#[test]
fn an_unchanged_frame_of_one_lineage_damages_nothing() -> Result<(), TestError> {
    let snapshot = scene()?;
    let shown = snapshot.at(Duration::ZERO);
    let mut damage = SurfaceDamage::default();
    let unchanged = inputs(snapshot.lineage());
    assert_eq!(
        damage.area(shown, viewport(), &unchanged, current())?,
        Some(whole())
    );
    assert_eq!(damage.area(shown, viewport(), &unchanged, current())?, None);
    Ok(())
}

#[test]
fn another_lineage_repaints_everything_without_reading_the_target() -> Result<(), TestError> {
    let snapshot = scene()?;
    let shown = snapshot.at(Duration::ZERO);
    let other = inputs(SceneTree::default().snapshot().lineage());
    let mut damage = SurfaceDamage::default();
    let _first = damage.area(shown, viewport(), &inputs(snapshot.lineage()), current())?;
    assert_eq!(
        damage.area(shown, viewport(), &other, unreadable)?,
        Some(whole())
    );
    assert_eq!(damage.area(shown, viewport(), &other, current())?, None);
    Ok(())
}

#[test]
fn lost_pixels_damage_the_whole_viewport() -> Result<(), TestError> {
    let snapshot = scene()?;
    let shown = snapshot.at(Duration::ZERO);
    let unchanged = inputs(snapshot.lineage());
    let mut damage = SurfaceDamage::default();
    let _first = damage.area(shown, viewport(), &unchanged, current())?;
    assert_eq!(
        damage.area(shown, viewport(), &unchanged, outdated())?,
        Some(whole())
    );
    Ok(())
}

#[test]
fn a_new_quality_scene_image_or_scale_damages_everything() -> Result<(), TestError> {
    let snapshot = scene()?;
    let shown = snapshot.at(Duration::ZERO);
    let unchanged = inputs(snapshot.lineage());
    let altered = [
        PixelInputs {
            quality: QualityChange::Changed,
            ..inputs(snapshot.lineage())
        },
        PixelInputs {
            scene_generation: 1,
            ..inputs(snapshot.lineage())
        },
        PixelInputs {
            scale: PixelScale::filling(viewport(), PixelSize::new(64, 30))?,
            ..inputs(snapshot.lineage())
        },
    ];
    for changed in altered {
        let mut damage = SurfaceDamage::default();
        let _first = damage.area(shown, viewport(), &unchanged, current())?;
        assert_eq!(
            damage.area(shown, viewport(), &changed, current())?,
            Some(whole())
        );
    }
    Ok(())
}

#[test]
fn a_frame_that_was_not_shown_damages_the_whole_viewport_next() -> Result<(), TestError> {
    let snapshot = scene()?;
    let shown = snapshot.at(Duration::ZERO);
    let unchanged = inputs(snapshot.lineage());
    for outcome in [FrameOutcome::Pending, FrameOutcome::Stale] {
        let mut damage = SurfaceDamage::default();
        let _first = damage.area(shown, viewport(), &unchanged, current())?;
        damage.forget_unless_shown(outcome);
        assert_eq!(
            damage.area(shown, viewport(), &unchanged, current())?,
            Some(whole())
        );
    }
    Ok(())
}
