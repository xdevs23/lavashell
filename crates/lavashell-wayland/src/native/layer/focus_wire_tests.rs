// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::focus_grab::protocol::{hyprland_focus_grab_manager_v1, hyprland_focus_grab_v1};
use crate::native::wire_messages::WireMessage;
use crate::test_error::TestError;
use lavashell_core::geometry::Size;
use lavashell_platform::{FocusGrabEvent, FocusGrabRequest};
use layer_client::{LAYER, LayerClient, top_bar};
use zwlr_layer_surface_v1::KeyboardInteractivity;

fn layer_client() -> Result<(LayerClient, ZwlrLayerSurfaceV1), TestError> {
    let mut client = LayerClient::new(4)?;
    let layer = client.layer(top_bar(Size::ZERO)?, KeyboardMode::OnDemand)?;
    assert_ne!(client.flush()?, Vec::new());
    Ok((client, layer))
}

fn destroy_grab(grab: u32) -> WireMessage {
    WireMessage::new(grab, hyprland_focus_grab_v1::REQ_DESTROY_OPCODE, &[])
}

#[test]
fn an_acquired_grab_adds_the_surface_until_the_runtime_releases_it() -> Result<(), TestError> {
    let (mut client, _layer) = layer_client()?;
    let manager = client.manager.id().protocol_id();
    let surface = client.surface.id().protocol_id();
    for _ in 0..2 {
        let started = client.execute(FocusGrabRequest::Acquire(LAYER))?;
        let grab = started[0].arguments[0];
        assert_eq!(
            started,
            vec![
                WireMessage::new(
                    manager,
                    hyprland_focus_grab_manager_v1::REQ_CREATE_GRAB_OPCODE,
                    &[grab]
                ),
                WireMessage::new(
                    grab,
                    hyprland_focus_grab_v1::REQ_ADD_SURFACE_OPCODE,
                    &[surface]
                ),
                WireMessage::new(grab, hyprland_focus_grab_v1::REQ_COMMIT_OPCODE, &[]),
            ]
        );
        let released = client.execute(FocusGrabRequest::Release(LAYER))?;
        assert_eq!(released, vec![destroy_grab(grab)]);
        assert_eq!(
            client.execute(FocusGrabRequest::Release(LAYER))?,
            Vec::new()
        );
    }
    Ok(())
}

#[test]
fn a_grab_cleared_by_the_compositor_is_destroyed_and_reported() -> Result<(), TestError> {
    let (mut client, _layer) = layer_client()?;
    let grab = client.execute(FocusGrabRequest::Acquire(LAYER))?[0].arguments[0];
    assert_eq!(client.clear_focus(), FocusGrabEvent::Cleared(LAYER));
    assert_eq!(client.flush()?, vec![destroy_grab(grab)]);
    assert_eq!(
        client.execute(FocusGrabRequest::Release(LAYER))?,
        Vec::new()
    );
    Ok(())
}

#[test]
fn a_clear_of_another_grab_leaves_the_grab_of_the_layer() -> Result<(), TestError> {
    let (mut client, _layer) = layer_client()?;
    let held = client.execute(FocusGrabRequest::Acquire(LAYER))?[0].arguments[0];
    let manager = client.manager.id().protocol_id();
    let (other, cleared) = client.clear_other_grab();
    assert_eq!(cleared, None);
    assert_eq!(
        client.flush()?,
        vec![WireMessage::new(
            manager,
            hyprland_focus_grab_manager_v1::REQ_CREATE_GRAB_OPCODE,
            &[other.id().protocol_id()]
        )]
    );
    let released = client.execute(FocusGrabRequest::Release(LAYER))?;
    assert_eq!(released, vec![destroy_grab(held)]);
    Ok(())
}

#[test]
fn keyboard_modes_follow_the_protocol_version_and_replay_for_a_new_role() -> Result<(), TestError> {
    for version in [3, 4] {
        let on_demand = if version >= 4 {
            KeyboardInteractivity::OnDemand
        } else {
            KeyboardInteractivity::Exclusive
        };
        let mut client = LayerClient::new(version)?;
        let layer = client.layer(top_bar(Size::ZERO)?, KeyboardMode::Off)?;
        let role = layer.id().protocol_id();
        let keyboard = |mode: KeyboardInteractivity| {
            WireMessage::new(
                role,
                zwlr_layer_surface_v1::REQ_SET_KEYBOARD_INTERACTIVITY_OPCODE,
                &[u32::from(mode)],
            )
        };
        assert!(
            client
                .flush_object(role)?
                .contains(&keyboard(KeyboardInteractivity::None))
        );
        apply_keyboard(&layer, KeyboardMode::OnDemand);
        assert_eq!(client.flush()?, vec![keyboard(on_demand)]);
        apply_keyboard(&layer, KeyboardMode::Off);
        assert_eq!(client.flush()?, vec![keyboard(KeyboardInteractivity::None)]);
        layer.destroy();
        client.replace_surface();
        assert_ne!(client.flush()?, Vec::new());
        let replacement = client.layer(top_bar(Size::ZERO)?, KeyboardMode::OnDemand)?;
        let role = replacement.id().protocol_id();
        let replayed = WireMessage::new(
            role,
            zwlr_layer_surface_v1::REQ_SET_KEYBOARD_INTERACTIVITY_OPCODE,
            &[u32::from(on_demand)],
        );
        assert!(client.flush_object(role)?.contains(&replayed));
    }
    Ok(())
}
