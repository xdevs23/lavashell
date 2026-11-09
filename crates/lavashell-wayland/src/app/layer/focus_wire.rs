// SPDX-License-Identifier: GPL-3.0-only
use super::test_error::LayerTestError;
use super::*;
use crate::ConnectionError;
use crate::Error;
use crate::app::control::{Control, RoleControl, SurfaceObjects};
use crate::app::focus_grab::Grabs;
use crate::app::wire_messages::messages;
use crate::app::wire_messages::test_error::WireTestError;
use crate::focus_grab::protocol::hyprland_focus_grab_manager_v1::HyprlandFocusGrabManagerV1;
use crate::focus_grab::protocol::hyprland_focus_grab_v1::HyprlandFocusGrabV1;
use crate::test_error::TestError;
use lavashell_core::geometry::Size;
use lavashell_core::panel::{
    AxisPlacement, ContentSize, ExclusiveZone, Extent, LayerLayout, LayerMargins,
};
use lavashell_platform::{FocusGrabEvent, FocusGrabRequest};
use sctk::reexports::client::Connection;
use sctk::reexports::client::protocol::wl_compositor;
use smithay_client_toolkit as sctk;
use std::os::unix::net::UnixStream;

/// The surface id of the layer in the runtime.
pub(super) const LAYER: SurfaceId = SurfaceId(0);

type Messages = Vec<(u32, u16, Vec<u32>)>;

/// The geometry of a bar across the top edge of the output, as tall as content of the size `content`, which reserves
/// its height.
pub(super) fn top_bar(content: Size) -> Result<PanelGeometry, TestError> {
    let layout = LayerLayout {
        margins: LayerMargins::default(),
        horizontal: AxisPlacement::Fill,
        vertical: AxisPlacement::Start(Extent::Content),
        exclusive_zone: ExclusiveZone::LayerExtent,
    };
    Ok(PanelGeometry::of(layout, ContentSize::try_from(content)?))
}

/// A layer and a focus grab manager whose requests a test reads back from the other end of a socket. The test passes
/// the surface states and the focus grab requests that the runtime sends.
pub(super) struct Wire {
    pub(super) connection: Connection,
    pub(super) peer: UnixStream,
    qh: QueueHandle<Translator>,
    pub(super) surface: wl_surface::WlSurface,
    shell: ZwlrLayerShellV1,
    pub(super) manager: HyprlandFocusGrabManagerV1,
    compositor: wl_compositor::WlCompositor,
    client_qh: QueueHandle<wire_tests::Client>,
    /// The control of the layer created last by `layer`.
    pub(super) control: Option<Control>,
    grabs: Grabs,
}

impl Wire {
    pub(super) fn new(version: u32) -> Result<Self, TestError> {
        let (socket, peer) = UnixStream::pair().map_err(TestError::CreateSocketPair)?;
        peer.set_nonblocking(true)
            .map_err(WireTestError::ConfigureSocket)?;
        let connection = Connection::from_socket(socket)
            .map_err(ConnectionError::Connect)
            .map_err(Error::from)?;
        let qh = connection.new_event_queue::<Translator>().handle();
        let client_qh = connection.new_event_queue::<wire_tests::Client>().handle();
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
            grabs: Grabs::new(manager.clone()),
            manager,
            compositor,
            client_qh,
            control: None,
        })
    }

    /// Creates the layer of a panel with `geometry` and `keyboard` on the surface, with the control of the layer.
    pub(super) fn layer(
        &mut self,
        geometry: PanelGeometry,
        keyboard: KeyboardMode,
    ) -> Result<Runtime, TestError> {
        let layout = ProtocolLayout::new(geometry)?;
        let level = LayerLevel::Top;
        let proxy = create(
            &self.shell,
            &self.surface,
            None,
            level,
            "wire",
            &self.qh,
            LAYER,
        );
        let objects = SurfaceObjects {
            connection: self.connection.clone(),
            wl: self.surface.clone(),
            viewport: None,
            presentation: None,
            compositor: self.compositor.clone(),
            background: None,
            role_control: RoleControl::Layer(LayerControl::new(proxy.clone(), layout)),
        };
        let control = Control::new(LAYER, objects, role_state(geometry), self.qh.clone());
        self.control = Some(control);
        Ok(Runtime::new(proxy, layout, keyboard))
    }

    /// Sends the requests for the parts of `state` that differ from the state applied last.
    pub(super) fn send_state(&mut self, state: &SurfaceState) -> Result<(), TestError> {
        let control = self.control.as_mut().ok_or(LayerTestError::NoLayer)?;
        control.send_state(state)?;
        Ok(())
    }

    pub(super) fn flush(&mut self) -> Result<Messages, TestError> {
        self.connection
            .flush()
            .map_err(ConnectionError::Flush)
            .map_err(Error::from)?;
        messages(&mut self.peer)
    }

    /// Executes `request` of the runtime on the grabs of the layer, and returns the requests sent.
    pub(super) fn execute(&mut self, request: FocusGrabRequest) -> Result<Messages, TestError> {
        let surface = &self.surface;
        self.grabs
            .execute(request, &self.qh, |_| Some(surface))
            .map_err(Error::from)?;
        self.flush()
    }

    /// Ends the grab of the layer as the compositor clears it, and returns the event reported for the clear.
    pub(super) fn clear_focus(&mut self) -> FocusGrabEvent {
        self.grabs.cleared(LAYER)
    }

    /// Ends the grab of the layer when the compositor clears another grab, and returns that grab with the event
    /// reported for the clear.
    pub(super) fn clear_other_grab(&mut self) -> (HyprlandFocusGrabV1, Option<FocusGrabEvent>) {
        let other = self.manager.create_grab(&self.qh, LAYER);
        let cleared = self.grabs.cleared_by(LAYER, &other);
        (other, cleared)
    }

    pub(super) fn replace_surface(&mut self) {
        self.surface.destroy();
        self.surface = self.compositor.create_surface(&self.client_qh, ());
    }
}
