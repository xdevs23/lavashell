// SPDX-License-Identifier: GPL-3.0-only
use super::control::RoleControl;
use super::{NativeRole, Surface, SurfaceProtocols, configure_event};
use crate::protocol_serial::ProtocolSerial;
use crate::sctk;
use crate::translator::Translator;
use lavashell_core::panel::{LayerLevel, PanelGeometry};
use lavashell_core::surface::{ConfigureToken, SurfaceChange, SurfaceId, SurfaceState};
use lavashell_platform::{KeyboardMode, PanelRole, SurfaceEvent};
use protocol_layout::apply;
use sctk::reexports::client::protocol::{wl_output, wl_surface};
use sctk::reexports::client::{Proxy, QueueHandle};
use sctk::reexports::protocols_wlr::layer_shell::v1::client::{
    zwlr_layer_shell_v1::{self, ZwlrLayerShellV1},
    zwlr_layer_surface_v1::{self, ZwlrLayerSurfaceV1},
};
use std::collections::VecDeque;
use std::num::NonZero;

mod error;
mod protocol_layout;

#[cfg(test)]
mod focus_wire_tests;
#[cfg(test)]
pub(in crate::app) mod layer_client;
#[cfg(test)]
mod state_wire_tests;
#[cfg(test)]
pub(crate) mod test_error;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod wire_tests;

pub use error::{LayerError, LayerLayoutError};
pub(super) use protocol_layout::ProtocolLayout;

pub(crate) struct LayerControl {
    proxy: ZwlrLayerSurfaceV1,
    layout: ProtocolLayout,
    configures: VecDeque<LayerConfigure>,
}

#[derive(Clone, Copy)]
pub struct LayerConfigure {
    pub token: ConfigureToken,
    pub serial: ProtocolSerial,
}

#[derive(Clone, Copy)]
pub(super) enum LayerRequest {
    Layout(ProtocolLayout),
    AckConfigure(ProtocolSerial),
}

impl LayerControl {
    pub(super) fn new(proxy: ZwlrLayerSurfaceV1, layout: ProtocolLayout) -> Self {
        Self {
            proxy,
            layout,
            configures: VecDeque::new(),
        }
    }

    pub(super) fn receive_configure(&mut self, configure: LayerConfigure) {
        self.configures.push_back(configure);
    }

    pub(super) fn request(
        &mut self,
        change: &SurfaceChange,
    ) -> Result<Option<LayerRequest>, LayerLayoutError> {
        Ok(match change {
            SurfaceChange::Panel(geometry) => {
                Some(LayerRequest::Layout(ProtocolLayout::new(*geometry)?))
            }
            SurfaceChange::Configure(token) => {
                self.forget_until(*token).map(LayerRequest::AckConfigure)
            }
            SurfaceChange::InputRegion(_) | SurfaceChange::BlurRegion(_) => None,
        })
    }

    pub(super) fn send(&mut self, request: LayerRequest) {
        match request {
            LayerRequest::Layout(next) => {
                apply(&self.proxy, Some(self.layout), next);
                self.layout = next;
            }
            LayerRequest::AckConfigure(serial) => self.proxy.ack_configure(serial.get()),
        }
    }

    fn forget_until(&mut self, token: ConfigureToken) -> Option<ProtocolSerial> {
        while let Some(&oldest) = self.configures.front() {
            if oldest.token == token {
                return Some(oldest.serial);
            }
            let _superseded = self.configures.pop_front();
        }
        None
    }
}

impl Surface {
    pub(crate) fn send_keyboard(
        &self,
        id: SurfaceId,
        keyboard: KeyboardMode,
    ) -> Result<(), LayerError> {
        let NativeRole::Layer(layer) = &self.role else {
            return Err(LayerError::PanelWithoutLayer(id));
        };
        apply_keyboard(layer, keyboard);
        Ok(())
    }
}

pub(crate) fn role_state(geometry: PanelGeometry) -> SurfaceState {
    let mut state = SurfaceState::default();
    state.apply(SurfaceChange::Panel(geometry));
    state
}

fn send_initial_state(proxy: &ZwlrLayerSurfaceV1, layout: ProtocolLayout, keyboard: KeyboardMode) {
    apply(proxy, None, layout);
    apply_keyboard(proxy, keyboard);
}

impl SurfaceProtocols {
    pub(crate) fn panel_role(
        &self,
        target: &LayerTarget,
        panel: &PanelRole,
        qh: &QueueHandle<Translator>,
    ) -> Result<(NativeRole, RoleControl), LayerError> {
        let layout = ProtocolLayout::new(panel.geometry).map_err(LayerError::Layout)?;
        let shell = (self.layers.as_ref()).ok_or(LayerError::NoLayerShell)?;
        let proxy = create(shell, target, panel, qh);
        send_initial_state(&proxy, layout, panel.keyboard);
        target.wl.commit();
        let control = LayerControl::new(proxy.clone(), layout);
        Ok((NativeRole::Layer(proxy), RoleControl::Layer(control)))
    }
}

pub(crate) fn translate_configure(
    serial: ProtocolSerial,
    width: u32,
    height: u32,
) -> (LayerConfigure, SurfaceEvent) {
    let token = ConfigureToken::default();
    let event = configure_event(NonZero::new(width), NonZero::new(height), Some(token));
    (LayerConfigure { token, serial }, event)
}

pub(crate) struct LayerTarget<'a> {
    pub(crate) surface: SurfaceId,
    pub(crate) wl: &'a wl_surface::WlSurface,
    pub(crate) output: Option<&'a wl_output::WlOutput>,
}

fn create(
    shell: &ZwlrLayerShellV1,
    target: &LayerTarget,
    panel: &PanelRole,
    qh: &QueueHandle<Translator>,
) -> ZwlrLayerSurfaceV1 {
    let layer = match panel.level {
        LayerLevel::Background => zwlr_layer_shell_v1::Layer::Background,
        LayerLevel::Bottom => zwlr_layer_shell_v1::Layer::Bottom,
        LayerLevel::Top => zwlr_layer_shell_v1::Layer::Top,
        LayerLevel::Overlay => zwlr_layer_shell_v1::Layer::Overlay,
    };
    shell.get_layer_surface(
        target.wl,
        target.output,
        layer,
        panel.namespace.clone(),
        qh,
        target.surface,
    )
}

// https://gitlab.freedesktop.org/wlroots/wlr-protocols/-/blob/master/unstable/wlr-layer-shell-unstable-v1.xml
const ON_DEMAND_KEYBOARD_SINCE: u32 = 4;

fn apply_keyboard(proxy: &ZwlrLayerSurfaceV1, keyboard: KeyboardMode) {
    use zwlr_layer_surface_v1::KeyboardInteractivity;
    proxy.set_keyboard_interactivity(match keyboard {
        KeyboardMode::Off => KeyboardInteractivity::None,
        KeyboardMode::OnDemand if proxy.version() >= ON_DEMAND_KEYBOARD_SINCE => {
            KeyboardInteractivity::OnDemand
        }
        KeyboardMode::OnDemand => KeyboardInteractivity::Exclusive,
    });
}
