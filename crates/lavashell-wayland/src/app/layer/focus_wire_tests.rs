// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::ConnectionError;
use crate::Error;
use crate::app::wire_messages::requests;
use crate::test_error::TestError;
use focus_wire::{LAYER, Wire, top_bar};
use lavashell_core::geometry::Size;
use lavashell_platform::{FocusGrabEvent, FocusGrabRequest};

/// A wire with a layer of a top bar on version 4 of layer-shell, whose creation requests were read already.
fn layer_wire() -> Result<(Wire, Runtime), TestError> {
    let mut wire = Wire::new(4)?;
    let layer = wire.layer(top_bar(Size::ZERO)?, KeyboardMode::OnDemand)?;
    assert!(!wire.flush()?.is_empty());
    Ok((wire, layer))
}

#[test]
fn private_wire_an_acquired_grab_adds_the_surface_until_the_runtime_releases_it()
-> Result<(), TestError> {
    let (mut wire, _layer) = layer_wire()?;
    let manager = wire.manager.id().protocol_id();
    let surface = wire.surface.id().protocol_id();
    for _ in 0..2 {
        let started = wire.execute(FocusGrabRequest::Acquire(LAYER))?;
        let grab = started[0].2[0];
        assert_eq!(
            started,
            vec![
                (manager, 0, vec![grab]),
                (grab, 0, vec![surface]),
                (grab, 2, vec![])
            ]
        );
        let released = wire.execute(FocusGrabRequest::Release(LAYER))?;
        assert_eq!(released, vec![(grab, 3, vec![])]);
        assert!(wire.execute(FocusGrabRequest::Release(LAYER))?.is_empty());
    }
    Ok(())
}

#[test]
fn private_wire_a_grab_cleared_by_the_compositor_is_destroyed_and_reported() -> Result<(), TestError>
{
    let (mut wire, _layer) = layer_wire()?;
    let grab = wire.execute(FocusGrabRequest::Acquire(LAYER))?[0].2[0];
    assert_eq!(wire.clear_focus(), FocusGrabEvent::Cleared(LAYER));
    assert_eq!(wire.flush()?, vec![(grab, 3, vec![])]);
    assert!(wire.execute(FocusGrabRequest::Release(LAYER))?.is_empty());
    Ok(())
}

#[test]
fn private_wire_a_clear_of_another_grab_leaves_the_grab_of_the_layer() -> Result<(), TestError> {
    let (mut wire, _layer) = layer_wire()?;
    let held = wire.execute(FocusGrabRequest::Acquire(LAYER))?[0].2[0];
    let manager = wire.manager.id().protocol_id();
    let (other, cleared) = wire.clear_other_grab();
    assert_eq!(cleared, None);
    assert_eq!(
        wire.flush()?,
        vec![(manager, 0, vec![other.id().protocol_id()])]
    );
    let released = wire.execute(FocusGrabRequest::Release(LAYER))?;
    assert_eq!(released, vec![(held, 3, vec![])]);
    Ok(())
}

#[test]
fn private_wire_keyboard_modes_follow_the_protocol_version_and_replay_for_a_new_role()
-> Result<(), TestError> {
    for version in [3, 4] {
        let on_demand = if version >= 4 { 2 } else { 1 };
        let mut wire = Wire::new(version)?;
        let layer = wire.layer(top_bar(Size::ZERO)?, KeyboardMode::Off)?;
        let role = layer.proxy.id().protocol_id();
        wire.connection
            .flush()
            .map_err(ConnectionError::Flush)
            .map_err(Error::from)?;
        assert!(requests(&mut wire.peer, role)?.contains(&(4, vec![0])));
        layer.send_keyboard(KeyboardMode::OnDemand);
        assert_eq!(wire.flush()?, vec![(role, 4, vec![on_demand])]);
        layer.send_keyboard(KeyboardMode::Off);
        assert_eq!(wire.flush()?, vec![(role, 4, vec![0])]);
        layer.proxy.destroy();
        wire.replace_surface();
        assert!(!wire.flush()?.is_empty());
        let replacement = wire.layer(top_bar(Size::ZERO)?, KeyboardMode::OnDemand)?;
        let role = replacement.proxy.id().protocol_id();
        wire.connection
            .flush()
            .map_err(ConnectionError::Flush)
            .map_err(Error::from)?;
        assert!(requests(&mut wire.peer, role)?.contains(&(4, vec![on_demand])));
    }
    Ok(())
}
