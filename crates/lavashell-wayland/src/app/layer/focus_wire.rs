// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::app::control::{Control, RoleControl, SurfaceObjects};
use crate::app::focus_grab::readiness;
use crate::focus_grab::protocol::hyprland_focus_grab_manager_v1::HyprlandFocusGrabManagerV1;
use crate::test_error::TestError;
use lavashell_core::geometry::Size;
use lavashell_core::scene::SceneBuilder;
use lavashell_renderer::{FrameOutcome, SurfaceControl};
use lavashell_runtime::{Choreographers, PanelContent, PanelRecords, PanelUpdate};
use lavashell_test_fixtures::NoText;
use sctk::reexports::client::Connection;
use sctk::reexports::client::protocol::wl_compositor;
use smithay_client_toolkit as sctk;
use std::os::unix::net::UnixStream;
use wire_tests::messages;

/// The surface id of the layer in the runtime.
pub(super) const LAYER: SurfaceId = SurfaceId(0);

pub(super) fn empty_view() -> View {
    View::new(lavashell_test_fixtures::facts(), |_| {})
}

/// A layer and a focus grab manager whose requests a test reads back from the other end of
/// a socket, with the panel of the layer in the runtime.
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
    panels: PanelRecords,
    choreographers: Choreographers,
    builder: SceneBuilder,
    state: SurfaceState,
    /// The control of the layer created last by `layer`.
    pub(super) control: Option<Control>,
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
            panels: PanelRecords::default(),
            choreographers: Choreographers::default(),
            builder: SceneBuilder::default(),
            state: SurfaceState::default(),
            control: None,
        })
    }

    /// Creates the layer of `options` with empty content, and opens its panel in the runtime in place of an
    /// earlier one.
    pub(super) fn layer(&mut self, options: &LayerSurface) -> Result<Runtime, TestError> {
        let panel = Panel::new(options.panel.clone(), &mut empty_view(), &mut NoText)
            .map_err(Error::from)?;
        let geometry = panel.geometry();
        let layout = ProtocolLayout::new(geometry)?;
        let proxy = create(&self.shell, &self.surface, None, options, &self.qh, LAYER);
        let objects = SurfaceObjects {
            wl: self.surface.clone(),
            viewport: None,
            presentation: None,
            compositor: self.compositor.clone(),
            background: None,
            role_control: RoleControl::Layer(LayerControl::new(proxy.clone(), layout)),
        };
        let control = Control::new(LAYER, objects, role_state(geometry), self.qh.clone());
        self.control = Some(control);
        self.choreographers = Choreographers::default();
        self.builder = SceneBuilder::default();
        self.state = SurfaceState::default();
        let layer = Runtime::new(proxy, options, layout, panel.keyboard_mode());
        self.panels.open(LAYER, panel);
        Ok(layer)
    }

    /// Sends the keyboard mode set by the runtime for the panel and the surface state with the geometry of the panel,
    /// as the frame of a layer does before its present.
    pub(super) fn update(&mut self, layer: &Runtime) -> Result<(), TestError> {
        send_keyboard(layer, self.panels.update(LAYER));
        self.build_state()?;
        self.send_state()
    }

    /// Sends the geometry of the panel to the sink of the layer and builds the surface state from the sink, as the
    /// app does before a frame or a commit without a frame.
    fn build_state(&mut self) -> Result<(), TestError> {
        send_surface_state(self.choreographers.sink(LAYER), &self.panels, LAYER, []);
        self.choreographers
            .build(LAYER, &mut self.builder, &mut self.state)?;
        Ok(())
    }

    /// Sends the requests for the parts of `geometry` that differ from the geometry applied last.
    pub(super) fn send_geometry(&mut self, geometry: PanelGeometry) -> Result<(), TestError> {
        self.state.apply(SurfaceChange::Panel(geometry));
        self.send_state()
    }

    fn send_state(&mut self) -> Result<(), TestError> {
        let control = self.control.as_mut().ok_or(TestError::NoLayer)?;
        control.send_state(&self.state)?;
        Ok(())
    }

    /// Runs the transaction of the panel with `view` while the host shows `content`, as the loop does before a
    /// render.
    pub(super) fn transaction(
        &mut self,
        layer: &Runtime,
        view: &mut View,
        content: ContentState,
    ) -> Result<Transaction, TestError> {
        let PanelUpdate {
            keyboard,
            transaction,
        } = self.panels.update_transaction(LAYER, content);
        send_keyboard(layer, keyboard);
        match transaction {
            Transaction::Settled => {}
            Transaction::NeedsFrame => view.invalidate(),
            Transaction::Committed => {
                self.build_state()?;
                let control = self.control.as_mut().ok_or(TestError::NoLayer)?;
                control.commit_state(&self.state)?;
            }
        }
        Ok(transaction)
    }

    pub(super) fn keyboard_granted(&self) -> bool {
        self.panels.get(LAYER).is_some_and(Panel::keyboard_granted)
    }

    pub(super) fn flush(&mut self) -> Result<Vec<(u32, u16, Vec<u32>)>, TestError> {
        self.connection.flush().map_err(Error::Flush)?;
        messages(&mut self.peer)
    }

    pub(super) fn commit(&mut self, layer: &Runtime) -> Result<Vec<(u16, Vec<u32>)>, TestError> {
        let transaction = self.transaction(layer, &mut empty_view(), ContentState::Current)?;
        let messages = self.flush()?;
        let surface = self.surface.id().protocol_id();
        let commits: Vec<_> = messages
            .iter()
            .filter(|(id, _, _)| *id == surface)
            .collect();
        if transaction == Transaction::Committed {
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
        let panel = self.panels.get(LAYER).ok_or(TestError::NoPanel)?;
        let readiness = readiness(panel, true, self.size);
        layer.update_focus_grab(
            Some(&self.manager),
            &self.surface,
            &self.qh,
            LAYER,
            readiness,
        );
        self.flush()
    }

    pub(super) fn present(
        &mut self,
        frame: FrameOutcome,
    ) -> Result<Vec<(u32, u16, Vec<u32>)>, TestError> {
        // This fixture has no renderer. The test commits the surface where the renderer commits a shown
        // frame, then reports the frame to the runtime, as the app does after the renderer presents.
        if frame == FrameOutcome::Shown {
            self.surface.commit();
            let content = PanelContent {
                size: self.size,
                input: self.input,
            };
            self.panels.shown(LAYER, content);
        }
        self.flush()
    }

    pub(super) fn replace_surface(&mut self) {
        self.surface.destroy();
        self.surface = self.compositor.create_surface(&self.client_qh, ());
    }
}

fn send_keyboard(layer: &Runtime, keyboard: Option<KeyboardMode>) {
    if let Some(keyboard) = keyboard {
        layer.send_keyboard(keyboard);
    }
}
