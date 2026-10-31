// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::app::{Connection, sctk};
use crate::focus_grab::protocol::hyprland_focus_grab_manager_v1::HyprlandFocusGrabManagerV1;
use crate::test_error::TestError;
use sctk::reexports::client::protocol::wl_compositor;
use std::os::unix::net::UnixStream;
use wire_tests::messages;

/// A layer and a focus grab manager whose requests a test reads back from the other end of
/// a socket.
pub(super) struct Wire {
    pub(super) connection: Connection,
    pub(super) peer: UnixStream,
    qh: QueueHandle<Platform>,
    pub(super) surface: wl_surface::WlSurface,
    shell: ZwlrLayerShellV1,
    pub(super) manager: HyprlandFocusGrabManagerV1,
    compositor: wl_compositor::WlCompositor,
    client_qh: QueueHandle<wire_tests::Client>,
    pub(super) size: Size,
    pub(super) input: bool,
}

impl Wire {
    pub(super) fn new(version: u32) -> Result<Self, TestError> {
        let (socket, peer) = UnixStream::pair().map_err(TestError::CreateSocketPair)?;
        peer.set_nonblocking(true)
            .map_err(TestError::ConfigureSocket)?;
        let connection = Connection::from_socket(socket).map_err(Error::Connect)?;
        let queue = connection.new_event_queue::<Platform>();
        let qh = queue.handle();
        let client_queue = connection.new_event_queue::<wire_tests::Client>();
        let client_qh = client_queue.handle();
        let registry = connection.display().get_registry(&client_qh, ());
        let compositor = registry.bind::<wl_compositor::WlCompositor, _, _>(1, 4, &client_qh, ());
        let shell = registry.bind::<ZwlrLayerShellV1, _, _>(2, version, &qh, ());
        let manager = registry.bind::<HyprlandFocusGrabManagerV1, _, _>(3, 1, &qh, ());
        let surface = compositor.create_surface(&client_qh, ());
        Ok(Self {
            connection,
            peer,
            qh,
            surface,
            shell,
            manager,
            compositor,
            client_qh,
            size: crate::logical::size((1, 1)),
            input: true,
        })
    }

    pub(super) fn layer(&mut self, options: &LayerSurface) -> Result<Runtime, TestError> {
        let context = LayerContext {
            content: Size::ZERO,
        };
        let layout = ProtocolLayout::new(options.current_layout(), context)?;
        let proxy = create(
            &self.shell,
            &self.surface,
            None,
            options,
            layout,
            &self.qh,
            SurfaceId(0),
        );
        Ok(Runtime::new(proxy, options, context, layout))
    }

    pub(super) fn flush(&mut self) -> Result<Vec<(u32, u16, Vec<u32>)>, TestError> {
        self.connection.flush().map_err(Error::Flush)?;
        messages(&mut self.peer)
    }

    pub(super) fn commit(
        &mut self,
        layer: &mut Runtime,
    ) -> Result<Vec<(u16, Vec<u32>)>, TestError> {
        layer.update();
        let changed = layer.commit(&self.surface, true);
        let messages = self.flush()?;
        let surface = self.surface.id().protocol_id();
        let commits: Vec<_> = messages
            .iter()
            .filter(|(id, _, _)| *id == surface)
            .collect();
        if changed {
            assert_eq!(commits, vec![&(surface, 6, vec![])]);
        } else {
            assert!(messages.is_empty());
        }
        let role = layer.proxy.id().protocol_id();
        Ok(messages
            .into_iter()
            .filter_map(|(id, opcode, words)| (id == role).then_some((opcode, words)))
            .collect())
    }

    pub(super) fn focus(
        &mut self,
        layer: &mut Runtime,
    ) -> Result<Vec<(u32, u16, Vec<u32>)>, TestError> {
        layer.update_focus_grab(
            Some(&self.manager),
            &self.surface,
            &self.qh,
            SurfaceId(0),
            true,
            self.size,
        );
        self.flush()
    }

    pub(super) fn present(
        &mut self,
        layer: &mut Runtime,
        frame: FrameOutcome,
    ) -> Result<Vec<(u32, u16, Vec<u32>)>, TestError> {
        // This fixture has no renderer. The test commits the surface where the renderer commits a shown
        // frame, then reports the frame through `presented`, as the app does after the renderer presents.
        if frame == FrameOutcome::Shown {
            self.surface.commit();
        }
        layer.presented(
            frame,
            Content {
                size: self.size,
                input: self.input,
            },
        );
        self.flush()
    }

    pub(super) fn replace_surface(&mut self) {
        self.surface.destroy();
        self.surface = self.compositor.create_surface(&self.client_qh, ());
    }
}
