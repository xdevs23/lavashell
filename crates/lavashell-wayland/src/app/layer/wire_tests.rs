// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::LayerMargins;
use crate::app::{Connection, delegate_noop, sctk};
use crate::test_error::TestError;
use lavashell_core::units::DpExt;
use sctk::reexports::client::protocol::{wl_compositor, wl_registry};
use std::{io::Read, os::unix::net::UnixStream};

pub(super) struct Client;

delegate_noop!(Client: ignore wl_registry::WlRegistry);

delegate_noop!(Client: ignore wl_compositor::WlCompositor);

delegate_noop!(Client: ignore wl_surface::WlSurface);

delegate_noop!(Client: ignore ZwlrLayerShellV1);

delegate_noop!(Client: ignore ZwlrLayerSurfaceV1);

pub(super) fn requests(peer: &mut UnixStream, id: u32) -> Result<Vec<(u16, Vec<u32>)>, TestError> {
    Ok(messages(peer)?
        .into_iter()
        .filter_map(|(object, opcode, words)| (object == id).then_some((opcode, words)))
        .collect())
}

pub(super) fn messages(peer: &mut UnixStream) -> Result<Vec<(u32, u16, Vec<u32>)>, TestError> {
    // The client flushed before this read, so every request it sent is in the socket already.
    let mut bytes = Vec::new();
    match peer.read_to_end(&mut bytes) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
        Err(error) => return Err(TestError::ReadProtocol(error)),
    }
    let word = |bytes: &[u8]| Ok::<_, TestError>(u32::from_ne_bytes(bytes.try_into()?));
    let mut offset = 0;
    let mut result = Vec::new();
    while offset < bytes.len() {
        let header = word(&bytes[offset + 4..offset + 8])?;
        let size = usize::try_from(header >> 16)?;
        assert!(size >= 8 && offset + size <= bytes.len());
        result.push((
            word(&bytes[offset..offset + 4])?,
            u16::try_from(header & u32::from(u16::MAX))?,
            bytes[offset + 8..offset + size]
                .as_chunks::<4>()
                .0
                .iter()
                .map(|bytes| u32::from_ne_bytes(*bytes))
                .collect(),
        ));
        offset += size;
    }
    Ok(result)
}

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
    let context = LayerContext {
        content: Size::ZERO,
    };
    let anchor = (Anchor::Top | Anchor::Left).bits();
    let initial = ProtocolLayout::new(layout, context)?;
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
    let moved = ProtocolLayout::new(
        LayerLayout {
            margins: LayerMargins {
                left: FiniteDp::try_new(-8.dp())?,
                ..layout.margins
            },
            ..layout
        },
        context,
    )?;
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
fn private_wire_resizes_and_reserves_a_layer_again_when_its_content_measures_differently()
-> Result<(), TestError> {
    let mut wire = super::focus_wire::Wire::new(4)?;
    let mut layer = wire.layer(&LayerSurface::new("content"))?;
    assert!(!wire.flush()?.is_empty());
    assert!(wire.commit(&mut layer)?.is_empty());
    assert_eq!(
        layer.content_proposal(),
        Some(Proposal {
            width: None,
            height: None
        })
    );
    layer.content_measured(Size::new(500.dp(), 40.dp()));
    assert_eq!(
        wire.commit(&mut layer)?,
        vec![(0, vec![0, 40]), (2, vec![40])]
    );
    layer.content_measured(Size::new(500.dp(), 40.dp()));
    assert!(wire.commit(&mut layer)?.is_empty());
    Ok(())
}
