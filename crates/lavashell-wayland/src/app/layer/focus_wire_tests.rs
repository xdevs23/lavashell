// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::LayerMargins;
use crate::test_error::TestError;
use focus_wire::Wire;
use lavashell_core::prelude::{
    Density, DpExt, Modifier, NonNegativeDp, TextMeasurer, TextStyle, Viewport,
};
use lavashell_test_platform::WithoutFailures;
use wire_tests::requests;

#[test]
fn private_wire_empty_submitted_input_cannot_grab_before_a_nonempty_frame_is_shown()
-> Result<(), TestError> {
    let options = LayerSurface::new("input-focus")
        .keyboard(true)
        .focus_grab(crate::FocusGrab::new(|| true, || {}));
    let mut wire = Wire::new(4)?;
    let mut layer = wire.layer(&options)?;
    assert!(!wire.flush()?.is_empty());
    wire.input = false;
    assert!(!wire.present(&mut layer, FrameOutcome::Shown)?.is_empty());
    assert!(layer.keyboard_enabled());
    assert!(wire.focus(&mut layer)?.is_empty());
    wire.input = true;
    for frame in [FrameOutcome::Pending, FrameOutcome::Stale] {
        assert!(wire.present(&mut layer, frame)?.is_empty());
        assert!(wire.focus(&mut layer)?.is_empty());
    }
    assert!(!wire.present(&mut layer, FrameOutcome::Shown)?.is_empty());
    let surface = wire.surface.id().protocol_id();
    let manager = wire.manager.id().protocol_id();
    let started = wire.focus(&mut layer)?;
    let grab = started[0].2[0];
    assert_eq!(
        started,
        vec![
            (manager, 0, vec![grab]),
            (grab, 0, vec![surface]),
            (grab, 2, vec![])
        ]
    );
    Ok(())
}

#[test]
fn private_wire_old_extent_blocks_acquisition_and_committed_empty_input_releases_a_live_grab()
-> Result<(), TestError> {
    let options = LayerSurface::new("extent-focus")
        .keyboard(true)
        .focus_grab(crate::FocusGrab::new(|| true, || {}));
    let mut wire = Wire::new(4)?;
    let mut layer = wire.layer(&options)?;
    assert!(!wire.flush()?.is_empty());
    assert!(!wire.present(&mut layer, FrameOutcome::Shown)?.is_empty());
    wire.size = crate::logical::size((321, 101));
    assert!(wire.focus(&mut layer)?.is_empty());
    assert!(wire.present(&mut layer, FrameOutcome::Pending)?.is_empty());
    assert!(wire.focus(&mut layer)?.is_empty());
    assert!(!wire.present(&mut layer, FrameOutcome::Shown)?.is_empty());
    let grab = wire.focus(&mut layer)?[0].2[0];
    wire.size = crate::logical::size((321, 141));
    wire.input = false;
    assert!(wire.present(&mut layer, FrameOutcome::Pending)?.is_empty());
    assert!(wire.focus(&mut layer)?.is_empty());
    assert!(!wire.present(&mut layer, FrameOutcome::Shown)?.is_empty());
    assert_eq!(wire.focus(&mut layer)?, vec![(grab, 3, vec![])]);
    assert!(wire.focus(&mut layer)?.is_empty());
    Ok(())
}

#[test]
fn private_wire_cleared_active_request_stays_blocked_across_empty_and_nonempty_submissions()
-> Result<(), TestError> {
    let active = State::new(true);
    let requested = active.clone();
    let options = LayerSurface::new("cleared-input")
        .keyboard(true)
        .focus_grab(crate::FocusGrab::new(move || requested.get(), || {}));
    let mut wire = Wire::new(4)?;
    let mut layer = wire.layer(&options)?;
    assert!(!wire.flush()?.is_empty());
    assert!(!wire.present(&mut layer, FrameOutcome::Shown)?.is_empty());
    let grab = wire.focus(&mut layer)?[0].2[0];
    layer
        .focus
        .as_mut()
        .ok_or(TestError::NoFocusGrab)?
        .cleared();
    assert_eq!(wire.flush()?, vec![(grab, 3, vec![])]);
    wire.input = false;
    assert!(!wire.present(&mut layer, FrameOutcome::Shown)?.is_empty());
    assert!(wire.focus(&mut layer)?.is_empty());
    wire.input = true;
    for frame in [
        FrameOutcome::Pending,
        FrameOutcome::Stale,
        FrameOutcome::Shown,
    ] {
        // Only a shown frame commits the surface.
        let sent = wire.present(&mut layer, frame)?;
        assert_eq!(sent.is_empty(), frame != FrameOutcome::Shown);
        assert!(wire.focus(&mut layer)?.is_empty());
        assert!(active.get());
    }
    active.set(false);
    assert!(wire.focus(&mut layer)?.is_empty());
    active.set(true);
    let resumed = wire.focus(&mut layer)?;
    let next = resumed[0].2[0];
    assert_eq!(
        resumed,
        vec![
            (wire.manager.id().protocol_id(), 0, vec![next]),
            (next, 0, vec![wire.surface.id().protocol_id()]),
            (next, 2, vec![]),
        ]
    );
    Ok(())
}

#[test]
fn private_wire_keyboard_changes_commit_without_geometry_and_replay_for_a_new_role()
-> Result<(), TestError> {
    for version in [3, 4] {
        let visible = State::new(false);
        let active = visible.clone();
        let options = LayerSurface::new("keyboard-wire")
            .placement(
                AxisPlacement::Start(Extent::dp(1.dp())?),
                AxisPlacement::Start(Extent::dp(1.dp())?),
            )
            .keyboard(true)
            .keyboard_when(move || active.get());
        let mut wire = Wire::new(version)?;
        let mut layer = wire.layer(&options)?;
        wire.connection.flush().map_err(Error::Flush)?;
        let initial = requests(&mut wire.peer, layer.proxy.id().protocol_id())?;
        assert!(initial.contains(&(4, vec![0])));
        assert!(!layer.keyboard_enabled());
        assert!(!wire.present(&mut layer, FrameOutcome::Shown)?.is_empty());
        visible.set(true);
        let enabled = if version >= 4 { 2 } else { 1 };
        assert_eq!(wire.commit(&mut layer)?, vec![(4, vec![enabled])]);
        assert!(layer.keyboard_enabled());
        assert!(wire.commit(&mut layer)?.is_empty());
        visible.set(false);
        assert!(!layer.keyboard_enabled());
        assert_eq!(wire.commit(&mut layer)?, vec![(4, vec![0])]);
        assert!(wire.commit(&mut layer)?.is_empty());
        layer.proxy.destroy();
        wire.replace_surface();
        assert!(!wire.flush()?.is_empty());
        visible.set(true);
        let mut replacement = wire.layer(&options)?;
        wire.connection.flush().map_err(Error::Flush)?;
        let replay = requests(&mut wire.peer, replacement.proxy.id().protocol_id())?;
        assert!(replay.contains(&(4, vec![enabled])));
        assert!(!replacement.keyboard_enabled());
        wire.surface.commit();
        assert!(!wire.flush()?.is_empty());
        assert!(!replacement.keyboard_enabled());
        assert!(
            !wire
                .present(&mut replacement, FrameOutcome::Shown)?
                .is_empty()
        );
        assert!(replacement.keyboard_enabled());
    }
    Ok(())
}

#[test]
fn private_wire_queued_activation_cannot_grab_until_the_content_transaction_is_submitted()
-> Result<(), TestError> {
    let visible = State::new(false);
    let active = visible.clone();
    let options = LayerSurface::new("pending-focus")
        .keyboard(true)
        .keyboard_when(move || active.get())
        .focus_grab(crate::FocusGrab::new(|| true, || {}));
    let mut wire = Wire::new(4)?;
    let mut layer = wire.layer(&options)?;
    assert!(!wire.flush()?.is_empty());
    assert!(!wire.present(&mut layer, FrameOutcome::Shown)?.is_empty());
    visible.set(true);
    layer.update();
    assert!(layer.pending);
    assert_eq!(
        wire.focus(&mut layer)?,
        vec![(layer.proxy.id().protocol_id(), 4, vec![2])]
    );
    for frame in [
        FrameOutcome::Pending,
        FrameOutcome::Stale,
        FrameOutcome::Pending,
    ] {
        assert!(wire.present(&mut layer, frame)?.is_empty());
        layer.update();
        assert!(layer.pending);
        assert!(!layer.commit(&wire.surface, false));
        assert!(wire.focus(&mut layer)?.is_empty());
    }
    let surface = wire.surface.id().protocol_id();
    assert_eq!(
        wire.present(&mut layer, FrameOutcome::Shown)?,
        vec![(surface, 6, vec![])]
    );
    let manager = wire.manager.id().protocol_id();
    let started = wire.focus(&mut layer)?;
    let grab = started[0].2[0];
    assert_eq!(
        started,
        vec![
            (manager, 0, vec![grab]),
            (grab, 0, vec![surface]),
            (grab, 2, vec![])
        ]
    );
    layer.update();
    assert!(!layer.pending);
    assert!(!layer.commit(&wire.surface, true));
    assert!(wire.flush()?.is_empty());
    Ok(())
}

#[test]
fn private_wire_pending_content_keeps_a_live_grab_but_false_releases_before_any_commit()
-> Result<(), TestError> {
    let visible = State::new(true);
    let active = visible.clone();
    let layout = State::new(crate::LayerLayout {
        horizontal: AxisPlacement::Start(Extent::dp(1.dp())?),
        vertical: AxisPlacement::Start(Extent::dp(1.dp())?),
        exclusive_zone: ExclusiveZone::Reserve(NonNegativeDp::ZERO),
        margins: LayerMargins::default(),
    });
    let options = LayerSurface::new("retained-focus")
        .layout_state(layout.clone())
        .keyboard(true)
        .keyboard_when(move || active.get())
        .focus_grab(crate::FocusGrab::new(|| true, || {}));
    let mut wire = Wire::new(4)?;
    let mut layer = wire.layer(&options)?;
    assert!(!wire.flush()?.is_empty());
    assert!(!wire.present(&mut layer, FrameOutcome::Shown)?.is_empty());
    let grab = wire.focus(&mut layer)?[0].2[0];
    let taller = AxisPlacement::Start(Extent::dp(2.dp())?);
    layout.update(|layout| layout.vertical = taller);
    layer.update();
    assert!(layer.pending);
    assert_eq!(
        wire.flush()?,
        vec![(layer.proxy.id().protocol_id(), 0, vec![1, 2])]
    );
    assert!(wire.present(&mut layer, FrameOutcome::Pending)?.is_empty());
    assert!(wire.focus(&mut layer)?.is_empty());
    visible.set(false);
    assert_eq!(wire.focus(&mut layer)?, vec![(grab, 3, vec![])]);
    layer.update();
    assert!(layer.pending);
    assert_eq!(
        wire.flush()?,
        vec![(layer.proxy.id().protocol_id(), 4, vec![0])]
    );
    visible.set(true);
    layer.update();
    assert!(layer.pending);
    assert_eq!(
        wire.focus(&mut layer)?,
        vec![(layer.proxy.id().protocol_id(), 4, vec![2])]
    );
    assert!(wire.present(&mut layer, FrameOutcome::Stale)?.is_empty());
    layer.update();
    assert!(layer.pending);
    assert!(!wire.present(&mut layer, FrameOutcome::Shown)?.is_empty());
    assert_eq!(wire.focus(&mut layer)?.len(), 3);
    Ok(())
}

#[test]
fn private_wire_initial_and_replacement_configure_commits_do_not_admit_focus()
-> Result<(), TestError> {
    let options = LayerSurface::new("initial-focus")
        .keyboard(true)
        .focus_grab(crate::FocusGrab::new(|| true, || {}));
    let mut wire = Wire::new(4)?;
    let mut layer = wire.layer(&options)?;
    assert!(!wire.flush()?.is_empty());
    for generation in 0..2 {
        wire.surface.commit();
        assert_eq!(
            wire.focus(&mut layer)?,
            vec![(wire.surface.id().protocol_id(), 6, vec![])]
        );
        layer.update();
        assert!(layer.pending);
        assert!(!layer.commit(&wire.surface, false));
        assert!(wire.present(&mut layer, FrameOutcome::Pending)?.is_empty());
        assert!(wire.focus(&mut layer)?.is_empty());
        assert!(!wire.present(&mut layer, FrameOutcome::Shown)?.is_empty());
        let grab = wire.focus(&mut layer)?[0].2[0];
        if generation == 0 {
            layer.proxy.destroy();
            drop(layer);
            assert!(wire.flush()?.contains(&(grab, 3, vec![])));
            wire.replace_surface();
            layer = wire.layer(&options)?;
            assert!(!wire.flush()?.is_empty());
        }
    }
    Ok(())
}

#[test]
fn private_wire_idle_role_changes_request_a_frame_when_suboptimal_contents_refuse_commit()
-> Result<(), TestError> {
    struct Metrics;
    impl TextMeasurer for Metrics {
        fn measure(&mut self, _: &str, _: &TextStyle<Dp>, _: Dp) -> Size {
            Size::ZERO
        }
    }
    let visible = State::new(false);
    let active = visible.clone();
    let options = LayerSurface::new("idle-focus")
        .keyboard(true)
        .keyboard_when(move || active.get())
        .focus_grab(crate::FocusGrab::new(|| true, || {}));
    let mut wire = Wire::new(4)?;
    let mut layer = wire.layer(&options)?;
    let mut view = View::new(lavashell_test_platform::facts(), |ui| {
        ui.spacer(1.dp(), 1.dp(), Modifier::new());
    });
    let viewport = Viewport::new(Size::new(1.dp(), 1.dp()), Density::ONE);
    // A spacer draws nothing.
    let update = (view.frame(viewport, &mut Metrics)).without_failures()?;
    assert!(update.changes.is_empty());
    assert!(!wire.flush()?.is_empty());
    assert!(!wire.present(&mut layer, FrameOutcome::Shown)?.is_empty());
    let mut grab = None;
    for (enabled, keyboard) in [(true, 2), (false, 0)] {
        assert!(!view.needs_visual_frame());
        visible.set(enabled);
        assert!(!view.needs_visual_frame());
        if let Some(grab) = grab.take() {
            assert_eq!(wire.focus(&mut layer)?, vec![(grab, 3, vec![])]);
        }
        // After a suboptimal frame is shown, the renderer reports the contents of the target as outdated. The
        // test passes that value, `false`, to `update_transaction` as the renderer would.
        layer.update_transaction(&wire.surface, &mut view, false);
        assert_eq!(
            wire.flush()?,
            vec![(layer.proxy.id().protocol_id(), 4, vec![keyboard])]
        );
        assert!(view.needs_visual_frame());
        assert!(wire.focus(&mut layer)?.is_empty());
        let update = (view.frame(viewport, &mut Metrics)).without_failures()?;
        assert!(update.changes.is_empty());
        assert!(!view.needs_visual_frame());
        assert_eq!(
            wire.present(&mut layer, FrameOutcome::Shown)?,
            vec![(wire.surface.id().protocol_id(), 6, vec![])]
        );
        let focused = wire.focus(&mut layer)?;
        if enabled {
            assert_eq!(focused.len(), 3);
            grab = Some(focused[0].2[0]);
        } else {
            assert!(focused.is_empty());
        }
        layer.update_transaction(&wire.surface, &mut view, false);
        assert!(!view.needs_visual_frame());
        assert!(wire.flush()?.is_empty());
    }
    Ok(())
}

#[test]
fn private_wire_hidden_layer_cannot_grab_and_releases_even_if_the_request_stays_active()
-> Result<(), TestError> {
    let visible = State::new(false);
    let active = visible.clone();
    let options = LayerSurface::new("focus-wire")
        .keyboard(true)
        .keyboard_when(move || active.get())
        .focus_grab(crate::FocusGrab::new(|| true, || {}));
    let mut wire = Wire::new(4)?;
    let mut layer = wire.layer(&options)?;
    assert!(!wire.flush()?.is_empty());
    assert!(!wire.present(&mut layer, FrameOutcome::Shown)?.is_empty());
    assert!(wire.focus(&mut layer)?.is_empty());
    visible.set(true);
    layer.update();
    assert!(layer.pending);
    assert_eq!(
        wire.focus(&mut layer)?,
        vec![(layer.proxy.id().protocol_id(), 4, vec![2])]
    );
    assert!(layer.commit(&wire.surface, true));
    assert_eq!(
        wire.flush()?,
        vec![(wire.surface.id().protocol_id(), 6, vec![])]
    );
    let manager = wire.manager.id().protocol_id();
    let surface = wire.surface.id().protocol_id();
    let started = wire.focus(&mut layer)?;
    let grab = started[0].2[0];
    assert_eq!(
        started,
        vec![
            (manager, 0, vec![grab]),
            (grab, 0, vec![surface]),
            (grab, 2, vec![])
        ]
    );
    visible.set(false);
    assert_eq!(wire.focus(&mut layer)?, vec![(grab, 3, vec![])]);
    assert_eq!(wire.commit(&mut layer)?, vec![(4, vec![0])]);
    assert!(wire.focus(&mut layer)?.is_empty());
    visible.set(true);
    assert!(!wire.commit(&mut layer)?.is_empty());
    assert_eq!(wire.focus(&mut layer)?.len(), 3);
    Ok(())
}
