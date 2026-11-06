// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::app::wire_messages::{messages, requests};
use crate::test_error::TestError;
use focus_wire::{LAYER, Wire};
use lavashell_core::geometry::Size;
use lavashell_core::panel::{ContentSize, Extent, LayerLayout, LayerMargins};
use lavashell_core::units::DpExt;
use sctk::reexports::client::protocol::{wl_compositor, wl_registry};
use sctk::reexports::client::{Connection, delegate_noop};
use smithay_client_toolkit as sctk;
use std::os::unix::net::UnixStream;

pub(super) struct Client;

delegate_noop!(Client: ignore wl_registry::WlRegistry);

delegate_noop!(Client: ignore wl_compositor::WlCompositor);

delegate_noop!(Client: ignore wl_surface::WlSurface);

delegate_noop!(Client: ignore ZwlrLayerShellV1);

delegate_noop!(Client: ignore ZwlrLayerSurfaceV1);

#[test]
fn private_wire_keeps_margin_updates_separate_and_replays_geometry_for_a_new_surface()
-> Result<(), TestError> {
    let (socket, mut peer) = UnixStream::pair().map_err(TestError::CreateSocketPair)?;
    peer.set_nonblocking(true)
        .map_err(TestError::ConfigureSocket)?;
    let connection = Connection::from_socket(socket).map_err(Error::Connect)?;
    let queue = connection.new_event_queue::<Client>();
    let qh = queue.handle();
    let registry = connection.display().get_registry(&qh, ());
    let compositor = registry.bind::<wl_compositor::WlCompositor, _, _>(1, 4, &qh, ());
    let shell = registry.bind::<ZwlrLayerShellV1, _, _>(2, 4, &qh, ());
    let surface = compositor.create_surface(&qh, ());
    let layer = shell.get_layer_surface(
        &surface,
        None,
        zwlr_layer_shell_v1::Layer::Overlay,
        "private-wire".into(),
        &qh,
        (),
    );
    connection.flush().map_err(Error::Flush)?;
    assert!(!messages(&mut peer)?.is_empty());
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
    connection.flush().map_err(Error::Flush)?;
    assert_eq!(
        requests(&mut peer, layer.id().protocol_id())?,
        vec![
            (0, vec![321, 345]),
            (1, vec![anchor]),
            (2, vec![u32::MAX]),
            (3, vec![48, 0, 0, 1592]),
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
    connection.flush().map_err(Error::Flush)?;
    assert_eq!(
        requests(&mut peer, layer.id().protocol_id())?,
        vec![(3, vec![48, 0, 0, (-8_i32).cast_unsigned()])]
    );
    layer.destroy();
    surface.destroy();
    let surface = compositor.create_surface(&qh, ());
    let replacement = shell.get_layer_surface(
        &surface,
        None,
        zwlr_layer_shell_v1::Layer::Overlay,
        "private-wire".into(),
        &qh,
        (),
    );
    apply(&replacement, None, moved);
    connection.flush().map_err(Error::Flush)?;
    assert_eq!(
        requests(&mut peer, replacement.id().protocol_id())?,
        vec![
            (0, vec![321, 345]),
            (1, vec![anchor]),
            (2, vec![u32::MAX]),
            (3, vec![48, 0, 0, (-8_i32).cast_unsigned()]),
        ]
    );
    Ok(())
}

#[test]
fn private_wire_resizes_and_reserves_a_layer_again_for_a_new_content_height()
-> Result<(), TestError> {
    let mut wire = Wire::new(4)?;
    let options = LayerSurface::new("content");
    let mut layer = wire.layer(&options)?;
    assert!(!wire.flush()?.is_empty());
    assert!(wire.commit(&mut layer)?.is_empty());
    let content = ContentSize::try_from(Size::new(500.dp(), 40.dp()))?;
    let geometry = PanelGeometry::of(options.panel.current_layout(), content);
    let role = layer.proxy.id().protocol_id();
    for sent in [vec![(0, vec![0, 40]), (2, vec![40])], Vec::new()] {
        layer.execute(&wire.surface, PanelRequest::Update(LAYER, geometry))?;
        wire.connection.flush().map_err(Error::Flush)?;
        assert_eq!(requests(&mut wire.peer, role)?, sent);
    }
    Ok(())
}
