// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::native::wire_messages::{FakeCompositor, WireMessage};
use crate::sctk;
use crate::test_error::TestError;
use lavashell_core::geometry::Size;
use lavashell_core::panel::{
    AxisPlacement, ContentSize, ExclusiveZone, Extent, LayerLayout, LayerMargins,
};
use lavashell_core::units::{DpExt, FiniteDp};
use layer_client::{LayerClient, top_bar};
use sctk::reexports::client::delegate_noop;
use sctk::reexports::client::protocol::{wl_compositor, wl_registry};
use zwlr_layer_surface_v1::Anchor;

pub(super) struct Client;

delegate_noop!(Client: ignore wl_registry::WlRegistry);

delegate_noop!(Client: ignore wl_compositor::WlCompositor);

delegate_noop!(Client: ignore wl_surface::WlSurface);

delegate_noop!(Client: ignore ZwlrLayerShellV1);

delegate_noop!(Client: ignore ZwlrLayerSurfaceV1);

#[test]
fn margin_updates_are_sent_separately_and_geometry_is_replayed_for_a_new_surface()
-> Result<(), TestError> {
    let mut compositor = FakeCompositor::new()?;
    let queue = compositor.connection.new_event_queue::<Client>();
    let qh = queue.handle();
    let registry = compositor.connection.display().get_registry(&qh, ());
    let wl_compositor = registry.bind::<wl_compositor::WlCompositor, _, _>(1, 4, &qh, ());
    let shell = registry.bind::<ZwlrLayerShellV1, _, _>(2, 4, &qh, ());
    let surface = wl_compositor.create_surface(&qh, ());
    let layer = shell.get_layer_surface(
        &surface,
        None,
        zwlr_layer_shell_v1::Layer::Overlay,
        "private-wire".into(),
        &qh,
        (),
    );
    assert_ne!(compositor.flush()?, []);
    let layout = LayerLayout {
        horizontal: AxisPlacement::Start(Extent::dp(321.dp())?),
        vertical: AxisPlacement::Start(Extent::dp(345.dp())?),
        exclusive_zone: ExclusiveZone::IgnoreOthers,
        margins: LayerMargins {
            top: FiniteDp::try_new(48.dp())?,
            left: FiniteDp::try_new(1592.dp())?,
            ..Default::default()
        },
    };
    let anchor = (Anchor::Top | Anchor::Left).bits();
    let empty = ContentSize::try_from(Size::ZERO)?;
    let initial = ProtocolLayout::new(PanelGeometry::of(layout, empty))?;
    apply(&layer, None, initial);
    let role = layer.id().protocol_id();
    let request = |opcode, arguments: &[u32]| WireMessage::new(role, opcode, arguments);
    assert_eq!(
        compositor.flush_object(role)?,
        vec![
            request(zwlr_layer_surface_v1::REQ_SET_SIZE_OPCODE, &[321, 345]),
            request(zwlr_layer_surface_v1::REQ_SET_ANCHOR_OPCODE, &[anchor]),
            request(
                zwlr_layer_surface_v1::REQ_SET_EXCLUSIVE_ZONE_OPCODE,
                &[u32::MAX]
            ),
            request(
                zwlr_layer_surface_v1::REQ_SET_MARGIN_OPCODE,
                &[48, 0, 0, 1592]
            ),
        ]
    );
    let moved = LayerLayout {
        margins: LayerMargins {
            left: FiniteDp::try_new(-8.dp())?,
            ..layout.margins
        },
        ..layout
    };
    let moved = ProtocolLayout::new(PanelGeometry::of(moved, empty))?;
    apply(&layer, Some(initial), moved);
    assert_eq!(
        compositor.flush_object(role)?,
        vec![request(
            zwlr_layer_surface_v1::REQ_SET_MARGIN_OPCODE,
            &[48, 0, 0, (-8_i32).cast_unsigned()]
        )]
    );
    layer.destroy();
    surface.destroy();
    let surface = wl_compositor.create_surface(&qh, ());
    let replacement = shell.get_layer_surface(
        &surface,
        None,
        zwlr_layer_shell_v1::Layer::Overlay,
        "private-wire".into(),
        &qh,
        (),
    );
    apply(&replacement, None, moved);
    let role = replacement.id().protocol_id();
    let request = |opcode, arguments: &[u32]| WireMessage::new(role, opcode, arguments);
    assert_eq!(
        compositor.flush_object(role)?,
        vec![
            request(zwlr_layer_surface_v1::REQ_SET_SIZE_OPCODE, &[321, 345]),
            request(zwlr_layer_surface_v1::REQ_SET_ANCHOR_OPCODE, &[anchor]),
            request(
                zwlr_layer_surface_v1::REQ_SET_EXCLUSIVE_ZONE_OPCODE,
                &[u32::MAX]
            ),
            request(
                zwlr_layer_surface_v1::REQ_SET_MARGIN_OPCODE,
                &[48, 0, 0, (-8_i32).cast_unsigned()]
            ),
        ]
    );
    Ok(())
}

#[test]
fn a_layer_is_resized_and_reserved_again_for_a_new_content_height() -> Result<(), TestError> {
    let mut client = LayerClient::new(4)?;
    let layer = client.layer(top_bar(Size::ZERO)?, KeyboardMode::Off)?;
    assert_ne!(client.flush()?, []);
    let mut state = SurfaceState::default();
    state.apply(SurfaceChange::Panel(top_bar(Size::new(500.dp(), 40.dp()))?));
    let role = layer.id().protocol_id();
    let request = |opcode, arguments: &[u32]| WireMessage::new(role, opcode, arguments);
    let resized = vec![
        request(zwlr_layer_surface_v1::REQ_SET_SIZE_OPCODE, &[0, 40]),
        request(zwlr_layer_surface_v1::REQ_SET_EXCLUSIVE_ZONE_OPCODE, &[40]),
    ];
    for sent in [resized, Vec::new()] {
        client.send_state(&state)?;
        assert_eq!(client.flush_object(role)?, sent);
    }
    Ok(())
}
