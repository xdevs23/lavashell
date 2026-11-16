// SPDX-License-Identifier: GPL-3.0-only
use super::test_error::LayerTestError;
use super::*;
use crate::Error;
use crate::focus_grab::protocol::hyprland_focus_grab_manager_v1::HyprlandFocusGrabManagerV1;
use crate::focus_grab::protocol::hyprland_focus_grab_v1::HyprlandFocusGrabV1;
use crate::native::SurfaceExtensions;
use crate::native::control::{Control, RoleControl, SurfaceObjects};
use crate::native::focus_grab::Grabs;
use crate::native::owned_wl_surface::OwnedWlSurface;
use crate::native::wire_messages::{FakeCompositor, WireMessage};
use crate::sctk;
use crate::test_error::TestError;
use lavashell_core::geometry::Size;
use lavashell_core::panel::{
    AxisPlacement, ContentSize, ExclusiveZone, Extent, LayerLayout, LayerMargins,
};
use lavashell_platform::{FocusGrabEvent, FocusGrabRequest};
use sctk::reexports::client::protocol::wl_compositor;
use std::sync::Arc;

pub(super) const LAYER: SurfaceId = SurfaceId::numbered(0);

pub(super) fn top_bar(content: Size) -> Result<PanelGeometry, TestError> {
    let layout = LayerLayout {
        margins: LayerMargins::default(),
        horizontal: AxisPlacement::Fill,
        vertical: AxisPlacement::Start(Extent::Content),
        exclusive_zone: ExclusiveZone::LayerExtent,
    };
    Ok(PanelGeometry::of(layout, ContentSize::try_from(content)?))
}

pub(in crate::native) struct LayerClient {
    pub(super) compositor: FakeCompositor,
    qh: QueueHandle<Translator>,
    pub(super) surface: wl_surface::WlSurface,
    shell: ZwlrLayerShellV1,
    pub(super) manager: HyprlandFocusGrabManagerV1,
    wl_compositor: wl_compositor::WlCompositor,
    client_qh: QueueHandle<wire_tests::Client>,
    pub(super) control: Option<Control>,
    grabs: Grabs,
}

impl LayerClient {
    pub(in crate::native) fn new(version: u32) -> Result<Self, TestError> {
        let compositor = FakeCompositor::new()?;
        let connection = &compositor.connection;
        let qh = connection.new_event_queue::<Translator>().handle();
        let client_qh = connection.new_event_queue::<wire_tests::Client>().handle();
        let registry = connection.display().get_registry(&client_qh, ());
        let wl_compositor =
            registry.bind::<wl_compositor::WlCompositor, _, _>(1, 4, &client_qh, ());
        let shell = registry.bind::<ZwlrLayerShellV1, _, _>(2, version, &qh, ());
        let manager = registry.bind::<HyprlandFocusGrabManagerV1, _, _>(3, 1, &qh, ());
        let surface = wl_compositor.create_surface(&client_qh, ());
        Ok(Self {
            compositor,
            qh,
            surface,
            shell,
            grabs: Grabs::new(manager.clone()),
            manager,
            wl_compositor,
            client_qh,
            control: None,
        })
    }

    pub(super) fn layer(
        &mut self,
        geometry: PanelGeometry,
        keyboard: KeyboardMode,
    ) -> Result<ZwlrLayerSurfaceV1, TestError> {
        let layout = ProtocolLayout::new(geometry)?;
        let panel = PanelRole {
            output: None,
            level: LayerLevel::Top,
            namespace: "wire".to_owned(),
            geometry,
            keyboard,
        };
        let target = LayerTarget {
            surface: LAYER,
            wl: &self.surface,
            output: None,
        };
        let proxy = create(&self.shell, &target, &panel, &self.qh);
        let objects = SurfaceObjects {
            connection: self.compositor.connection.clone(),
            wl: self.surface.clone(),
            viewport: None,
            presentation: None,
            compositor: self.wl_compositor.clone(),
            background: None,
            role_control: RoleControl::Layer(LayerControl::new(proxy.clone(), layout)),
        };
        let control = Control::new(LAYER, objects, role_state(geometry), self.qh.clone());
        self.control = Some(control);
        send_initial_state(&proxy, layout, keyboard);
        Ok(proxy)
    }

    pub(in crate::native) fn native_surface(&mut self) -> Result<(Surface, u32), TestError> {
        let layer = self.layer(top_bar(Size::ZERO)?, KeyboardMode::Off)?;
        let role = layer.id().protocol_id();
        let backend = self.compositor.connection.backend();
        let surface = Surface {
            accessibility: None,
            extensions: SurfaceExtensions {
                viewport: None,
                fractional: None,
                background: None,
            },
            role: NativeRole::Layer(layer),
            window: Arc::new(OwnedWlSurface::new(self.surface.clone(), backend)),
        };
        Ok((surface, role))
    }

    pub(super) fn send_state(&mut self, state: &SurfaceState) -> Result<(), TestError> {
        let control = self.control.as_mut().ok_or(LayerTestError::NoLayer)?;
        control.send_state(state)?;
        Ok(())
    }

    pub(in crate::native) fn flush(&mut self) -> Result<Vec<WireMessage>, TestError> {
        self.compositor.flush()
    }

    pub(super) fn flush_object(&mut self, object: u32) -> Result<Vec<WireMessage>, TestError> {
        self.compositor.flush_object(object)
    }

    pub(super) fn execute(
        &mut self,
        request: FocusGrabRequest,
    ) -> Result<Vec<WireMessage>, TestError> {
        let surface = &self.surface;
        self.grabs
            .execute(request, &self.qh, |_| Some(surface))
            .map_err(Error::from)?;
        self.flush()
    }

    pub(super) fn clear_focus(&mut self) -> FocusGrabEvent {
        self.grabs.release_and_report_cleared(LAYER)
    }

    pub(super) fn clear_other_grab(&mut self) -> (HyprlandFocusGrabV1, Option<FocusGrabEvent>) {
        let other = self.manager.create_grab(&self.qh, LAYER);
        let cleared = self.grabs.release_if_cleared_by(LAYER, &other);
        (other, cleared)
    }

    pub(super) fn replace_surface(&mut self) {
        self.surface.destroy();
        self.surface = self.wl_compositor.create_surface(&self.client_qh, ());
    }
}
