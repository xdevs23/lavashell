// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::app::wire_messages::requests;
use crate::test_error::TestError;
use focus_wire::{Wire, empty_view};
use lavashell_core::panel::{Extent, LayerMargins};
use lavashell_core::prelude::{Density, DpExt, Modifier, NonNegativeDp, Size, State, Viewport};
use lavashell_renderer::FrameOutcome;
use lavashell_test_fixtures::{NoText, RecordedScene, WithoutFailures};
use std::cell::Cell;
use std::rc::Rc;

#[test]
fn private_wire_empty_submitted_input_cannot_grab_before_a_nonempty_frame_is_shown()
-> Result<(), TestError> {
    let options = LayerSurface::new("input-focus")
        .keyboard(true)
        .focus_grab(crate::FocusGrab::new(|| true, || {}));
    let mut wire = Wire::new(4)?;
    drop(wire.layer(&options)?);
    assert!(!wire.flush()?.is_empty());
    wire.input = false;
    assert!(!wire.present(FrameOutcome::Shown)?.is_empty());
    assert!(wire.keyboard_granted());
    assert!(wire.focus()?.is_empty());
    wire.input = true;
    for frame in [FrameOutcome::Pending, FrameOutcome::Stale] {
        assert!(wire.present(frame)?.is_empty());
        assert!(wire.focus()?.is_empty());
    }
    assert!(!wire.present(FrameOutcome::Shown)?.is_empty());
    let surface = wire.surface.id().protocol_id();
    let manager = wire.manager.id().protocol_id();
    let started = wire.focus()?;
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
    drop(wire.layer(&options)?);
    assert!(!wire.flush()?.is_empty());
    assert!(!wire.present(FrameOutcome::Shown)?.is_empty());
    wire.resize((321, 101));
    assert!(wire.focus()?.is_empty());
    assert!(wire.present(FrameOutcome::Pending)?.is_empty());
    assert!(wire.focus()?.is_empty());
    assert!(!wire.present(FrameOutcome::Shown)?.is_empty());
    let grab = wire.focus()?[0].2[0];
    wire.resize((321, 141));
    wire.input = false;
    assert!(wire.present(FrameOutcome::Pending)?.is_empty());
    assert!(wire.focus()?.is_empty());
    assert!(!wire.present(FrameOutcome::Shown)?.is_empty());
    assert_eq!(wire.focus()?, vec![(grab, 3, vec![])]);
    assert!(wire.focus()?.is_empty());
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
    drop(wire.layer(&options)?);
    assert!(!wire.flush()?.is_empty());
    assert!(!wire.present(FrameOutcome::Shown)?.is_empty());
    let grab = wire.focus()?[0].2[0];
    wire.clear_focus();
    assert_eq!(wire.flush()?, vec![(grab, 3, vec![])]);
    wire.input = false;
    assert!(!wire.present(FrameOutcome::Shown)?.is_empty());
    assert!(wire.focus()?.is_empty());
    wire.input = true;
    for frame in [
        FrameOutcome::Pending,
        FrameOutcome::Stale,
        FrameOutcome::Shown,
    ] {
        // Only a shown frame commits the surface.
        let sent = wire.present(frame)?;
        assert_eq!(sent.is_empty(), frame != FrameOutcome::Shown);
        assert!(wire.focus()?.is_empty());
        assert!(active.get());
    }
    active.set(false);
    assert!(wire.focus()?.is_empty());
    active.set(true);
    let resumed = wire.focus()?;
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
fn private_wire_clear_of_another_grab_leaves_the_held_grab_and_its_callback_alone()
-> Result<(), TestError> {
    let cleared = Rc::new(Cell::new(0_u32));
    let counted = cleared.clone();
    let grab = crate::FocusGrab::new(|| true, move || counted.set(counted.get() + 1));
    let options = LayerSurface::new("other-grab")
        .keyboard(true)
        .focus_grab(grab);
    let mut wire = Wire::new(4)?;
    drop(wire.layer(&options)?);
    assert!(!wire.flush()?.is_empty());
    assert!(!wire.present(FrameOutcome::Shown)?.is_empty());
    let held = wire.focus()?[0].2[0];
    let manager = wire.manager.id().protocol_id();
    let other = wire.clear_focus_from_other_grab().id().protocol_id();
    assert_eq!(wire.flush()?, vec![(manager, 0, vec![other])]);
    assert_eq!(cleared.get(), 0);
    assert!(wire.focus()?.is_empty());
    wire.clear_focus();
    assert_eq!(wire.flush()?, vec![(held, 3, vec![])]);
    assert_eq!(cleared.get(), 1);
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
        assert!(!wire.keyboard_granted());
        assert!(!wire.present(FrameOutcome::Shown)?.is_empty());
        visible.set(true);
        let enabled = if version >= 4 { 2 } else { 1 };
        assert_eq!(wire.commit(&mut layer)?, vec![(4, vec![enabled])]);
        assert!(wire.keyboard_granted());
        assert!(wire.commit(&mut layer)?.is_empty());
        visible.set(false);
        assert!(!wire.keyboard_granted());
        assert_eq!(wire.commit(&mut layer)?, vec![(4, vec![0])]);
        assert!(wire.commit(&mut layer)?.is_empty());
        layer.proxy.destroy();
        wire.replace_surface();
        assert!(!wire.flush()?.is_empty());
        visible.set(true);
        let replacement = wire.layer(&options)?;
        wire.connection.flush().map_err(Error::Flush)?;
        let replay = requests(&mut wire.peer, replacement.proxy.id().protocol_id())?;
        assert!(replay.contains(&(4, vec![enabled])));
        assert!(!wire.keyboard_granted());
        wire.surface.commit();
        assert!(!wire.flush()?.is_empty());
        assert!(!wire.keyboard_granted());
        assert!(!wire.present(FrameOutcome::Shown)?.is_empty());
        assert!(wire.keyboard_granted());
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
    assert!(!wire.present(FrameOutcome::Shown)?.is_empty());
    visible.set(true);
    wire.update(&mut layer)?;
    assert!(!wire.keyboard_granted());
    assert_eq!(
        wire.focus()?,
        vec![(layer.proxy.id().protocol_id(), 4, vec![2])]
    );
    for frame in [
        FrameOutcome::Pending,
        FrameOutcome::Stale,
        FrameOutcome::Pending,
    ] {
        assert!(wire.present(frame)?.is_empty());
        assert_eq!(
            wire.transaction(&mut layer, &mut empty_view(), ContentState::Stale)?,
            Transaction::NeedsFrame
        );
        assert!(wire.focus()?.is_empty());
    }
    let surface = wire.surface.id().protocol_id();
    assert_eq!(
        wire.present(FrameOutcome::Shown)?,
        vec![(surface, 6, vec![])]
    );
    let manager = wire.manager.id().protocol_id();
    let started = wire.focus()?;
    let grab = started[0].2[0];
    assert_eq!(
        started,
        vec![
            (manager, 0, vec![grab]),
            (grab, 0, vec![surface]),
            (grab, 2, vec![])
        ]
    );
    assert_eq!(
        wire.transaction(&mut layer, &mut empty_view(), ContentState::Current)?,
        Transaction::Settled
    );
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
    assert!(!wire.present(FrameOutcome::Shown)?.is_empty());
    let grab = wire.focus()?[0].2[0];
    let taller = AxisPlacement::Start(Extent::dp(2.dp())?);
    layout.update(|layout| layout.vertical = taller);
    wire.update(&mut layer)?;
    assert_eq!(
        wire.flush()?,
        vec![(layer.proxy.id().protocol_id(), 0, vec![1, 2])]
    );
    assert!(wire.present(FrameOutcome::Pending)?.is_empty());
    assert!(wire.focus()?.is_empty());
    visible.set(false);
    assert_eq!(wire.focus()?, vec![(grab, 3, vec![])]);
    wire.update(&mut layer)?;
    assert_eq!(
        wire.flush()?,
        vec![(layer.proxy.id().protocol_id(), 4, vec![0])]
    );
    visible.set(true);
    wire.update(&mut layer)?;
    assert_eq!(
        wire.focus()?,
        vec![(layer.proxy.id().protocol_id(), 4, vec![2])]
    );
    assert!(wire.present(FrameOutcome::Stale)?.is_empty());
    wire.update(&mut layer)?;
    assert!(!wire.present(FrameOutcome::Shown)?.is_empty());
    assert_eq!(wire.focus()?.len(), 3);
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
            wire.focus()?,
            vec![(wire.surface.id().protocol_id(), 6, vec![])]
        );
        assert_eq!(
            wire.transaction(&mut layer, &mut empty_view(), ContentState::Stale)?,
            Transaction::NeedsFrame
        );
        assert!(wire.present(FrameOutcome::Pending)?.is_empty());
        assert!(wire.focus()?.is_empty());
        assert!(!wire.present(FrameOutcome::Shown)?.is_empty());
        let grab = wire.focus()?[0].2[0];
        if generation == 0 {
            wire.retire(&layer)?;
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
    let visible = State::new(false);
    let active = visible.clone();
    let options = LayerSurface::new("idle-focus")
        .keyboard(true)
        .keyboard_when(move || active.get())
        .focus_grab(crate::FocusGrab::new(|| true, || {}));
    let mut wire = Wire::new(4)?;
    let mut layer = wire.layer(&options)?;
    let mut view = View::new(lavashell_test_fixtures::facts(), |ui| {
        ui.spacer(1.dp(), 1.dp(), Modifier::new());
    });
    let viewport = Viewport::new(Size::new(1.dp(), 1.dp()), Density::ONE);
    // A spacer draws nothing.
    let mut scene = RecordedScene::default();
    (view.frame_into(viewport, &mut NoText, &mut scene)).without_failures()?;
    assert!(scene.changes().is_empty());
    assert!(!wire.flush()?.is_empty());
    assert!(!wire.present(FrameOutcome::Shown)?.is_empty());
    let mut grab = None;
    for (enabled, keyboard) in [(true, 2), (false, 0)] {
        assert!(!view.needs_visual_frame());
        visible.set(enabled);
        assert!(!view.needs_visual_frame());
        if let Some(grab) = grab.take() {
            assert_eq!(wire.focus()?, vec![(grab, 3, vec![])]);
        }
        // After a suboptimal frame is shown, the renderer reports the contents of the target as outdated. The
        // test passes `ContentState::Stale` to the transaction as the loop would.
        assert_eq!(
            wire.transaction(&mut layer, &mut view, ContentState::Stale)?,
            Transaction::NeedsFrame
        );
        assert_eq!(
            wire.flush()?,
            vec![(layer.proxy.id().protocol_id(), 4, vec![keyboard])]
        );
        assert!(view.needs_visual_frame());
        assert!(wire.focus()?.is_empty());
        (view.frame_into(viewport, &mut NoText, &mut scene)).without_failures()?;
        assert!(scene.changes().is_empty());
        assert!(!view.needs_visual_frame());
        assert_eq!(
            wire.present(FrameOutcome::Shown)?,
            vec![(wire.surface.id().protocol_id(), 6, vec![])]
        );
        let focused = wire.focus()?;
        if enabled {
            assert_eq!(focused.len(), 3);
            grab = Some(focused[0].2[0]);
        } else {
            assert!(focused.is_empty());
        }
        assert_eq!(
            wire.transaction(&mut layer, &mut view, ContentState::Stale)?,
            Transaction::Settled
        );
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
    assert!(!wire.present(FrameOutcome::Shown)?.is_empty());
    assert!(wire.focus()?.is_empty());
    visible.set(true);
    wire.update(&mut layer)?;
    assert_eq!(
        wire.focus()?,
        vec![(layer.proxy.id().protocol_id(), 4, vec![2])]
    );
    assert_eq!(
        wire.transaction(&mut layer, &mut empty_view(), ContentState::Current)?,
        Transaction::Committed
    );
    assert_eq!(
        wire.flush()?,
        vec![(wire.surface.id().protocol_id(), 6, vec![])]
    );
    let manager = wire.manager.id().protocol_id();
    let surface = wire.surface.id().protocol_id();
    let started = wire.focus()?;
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
    assert_eq!(wire.focus()?, vec![(grab, 3, vec![])]);
    assert_eq!(wire.commit(&mut layer)?, vec![(4, vec![0])]);
    assert!(wire.focus()?.is_empty());
    visible.set(true);
    assert!(!wire.commit(&mut layer)?.is_empty());
    assert_eq!(wire.focus()?.len(), 3);
    Ok(())
}
