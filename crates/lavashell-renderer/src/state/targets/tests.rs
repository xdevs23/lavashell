// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::state::tests::{scene, viewport};
use crate::test_error::TestError;
use crate::test_surfaces::WIDE_SURFACE;
use lavashell_core::prelude::SceneTree;
use std::time::Duration;

fn whole() -> Rect {
    Rect::from_size(viewport().size)
}

fn inputs(lineage: TreeId) -> PixelInputs {
    PixelInputs {
        lineage,
        scene_generation: 0,
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
    let _damage_of_the_first_frame_is_not_checked =
        damage.area(shown, viewport(), &inputs(snapshot.lineage()), current())?;
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
    let _damage_of_the_first_frame_is_not_checked =
        damage.area(shown, viewport(), &unchanged, current())?;
    assert_eq!(
        damage.area(shown, viewport(), &unchanged, outdated())?,
        Some(whole())
    );
    Ok(())
}

#[test]
fn a_new_quality_or_scene_image_damages_everything() -> Result<(), TestError> {
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
    ];
    for changed in altered {
        let mut damage = SurfaceDamage::default();
        let _damage_of_the_first_frame_is_not_checked =
            damage.area(shown, viewport(), &unchanged, current())?;
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
        let _damage_of_the_first_frame_is_not_checked =
            damage.area(shown, viewport(), &unchanged, current())?;
        damage.forget_unless_shown(outcome);
        assert_eq!(
            damage.area(shown, viewport(), &unchanged, current())?,
            Some(whole())
        );
    }
    Ok(())
}

#[test]
fn a_new_scale_damages_everything_and_an_unchanged_one_nothing() -> Result<(), TestError> {
    let snapshot = scene()?;
    let shown = snapshot.at(Duration::ZERO);
    let unchanged = inputs(snapshot.lineage());
    let wide = PixelScale::filling(viewport(), WIDE_SURFACE.pixel_size())?;
    let mut damage = SurfaceDamage::default();
    damage.draw_at(PixelScale::SQUARE);
    let _damage_of_the_first_frame_is_not_checked =
        damage.area(shown, viewport(), &unchanged, current())?;
    damage.draw_at(PixelScale::SQUARE);
    assert_eq!(damage.area(shown, viewport(), &unchanged, current())?, None);
    damage.draw_at(wide);
    assert_eq!(
        damage.area(shown, viewport(), &unchanged, current())?,
        Some(whole())
    );
    Ok(())
}
