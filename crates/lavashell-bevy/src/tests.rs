// SPDX-License-Identifier: GPL-3.0-only
use super::{OVERLAY_SURFACE, OverlayFrame, SentFrame};
use crate::error::ShowFrameError;
use crate::test_error::TestError;
use lavashell_actor::JoinError;
use lavashell_core::color::Color;
use lavashell_core::frame::FrameTime;
use lavashell_core::geometry::{Size, Viewport};
use lavashell_core::scene::ChangeSink;
use lavashell_core::scene::tree::{ApplyError, Change, SceneNodeId};
use lavashell_core::ui::Modifier;
use lavashell_core::units::{Density, DpExt};
use lavashell_core::view::View;
use lavashell_runtime::scheduler::FrameSequence;
use lavashell_test_fixtures::NoText;
use std::sync::Arc;
use std::time::Duration;

fn square() -> View {
    View::new(lavashell_test_fixtures::facts(), |ui| {
        ui.spacer(20.dp(), 20.dp(), Modifier::new().background(Color::WHITE));
    })
}

fn show_frame(overlay: &mut OverlayFrame, view: &mut View) -> Result<(), ShowFrameError> {
    let viewport = Viewport::new(Size::new(100.dp(), 100.dp()), Density::ONE);
    overlay.show_frame_at(view, viewport, &mut NoText, FrameTime::default())
}

#[test]
fn a_built_scene_is_painted_with_the_number_of_its_own_frame() -> Result<(), TestError> {
    let mut overlay = OverlayFrame::start()?;
    let mut view = square();
    overlay.sequence = 3;
    show_frame(&mut overlay, &mut view)?;
    overlay.scene.stop()?;
    let unbuilt = SentFrame {
        sequence: 4,
        density: Density::ONE,
        presentation: Duration::ZERO,
    };
    let _no_frame_before = overlay.sent.insert(FrameSequence(1), unbuilt);
    let extracted = overlay.extracted();
    assert!(!extracted.built.scene.is_empty());
    assert!(extracted.sent.is_some_and(|sent| sent.sequence == 3));
    Ok(())
}

#[test]
fn a_restart_report_gives_the_next_frame_a_new_sink_once() -> Result<(), TestError> {
    let mut overlay = OverlayFrame::start()?;
    let mut view = square();
    show_frame(&mut overlay, &mut view)?;
    let first = overlay.scene.sink(OVERLAY_SURFACE, None).id();
    let unknown = SceneNodeId(u64::MAX);
    overlay.restart.publish(ApplyError::UnknownNode(unknown));
    assert!(overlay.needs_frame(&view));
    show_frame(&mut overlay, &mut view)?;
    let restarted = overlay.scene.sink(OVERLAY_SURFACE, None).id();
    assert_ne!(restarted, first);
    show_frame(&mut overlay, &mut view)?;
    assert_eq!(overlay.scene.sink(OVERLAY_SURFACE, None).id(), restarted);
    overlay.scene.stop()?;
    Ok(())
}

#[test]
fn every_frame_after_the_scene_builder_failed_returns_its_failure() -> Result<(), TestError> {
    let mut overlay = OverlayFrame::start()?;
    let mut sink = overlay.scene.sink(OVERLAY_SURFACE, None);
    sink.change(Change::Remove {
        id: SceneNodeId(u64::MAX),
    })?;
    sink.end_frame()?;
    let stopped = overlay.scene.stop().err();
    assert!(matches!(
        stopped.as_deref(),
        Some(lavashell_runtime::scene_pipeline::Error::RunBuilder(
            JoinError::Failed(_)
        ))
    ));
    let mut view = square();
    for _attempt in 0..2 {
        let shown = show_frame(&mut overlay, &mut view);
        assert!(matches!(
            shown,
            Err(ShowFrameError::Scene(failure))
                if stopped.as_ref().is_some_and(|stopped| Arc::ptr_eq(stopped, &failure))
        ));
    }
    Ok(())
}
