// SPDX-License-Identifier: GPL-3.0-only
use super::control::RoleControl;
use super::{NativeRole, Surface, configure_event};
use crate::translator::Translator;
use lavashell_core::panel::{LayerLevel, PanelGeometry};
use lavashell_core::surface::{ConfigureToken, SurfaceChange, SurfaceId, SurfaceState};
use lavashell_platform::{KeyboardMode, PanelRequest, PanelRole, SurfaceEvent};
use protocol_layout::apply;
use sctk::reexports::client::protocol::{wl_output, wl_surface};
use sctk::reexports::client::{Proxy, QueueHandle};
use sctk::reexports::protocols_wlr::layer_shell::v1::client::{
    zwlr_layer_shell_v1::{self, ZwlrLayerShellV1},
    zwlr_layer_surface_v1::{self, ZwlrLayerSurfaceV1},
};
use smithay_client_toolkit as sctk;
use std::collections::VecDeque;
use std::num::NonZero;

pub use error::{LayerError, LayerLayoutError};
pub(super) use protocol_layout::ProtocolLayout;

/// The layer-shell role of a surface.
pub(super) struct Runtime {
    pub proxy: ZwlrLayerSurfaceV1,
}

impl Runtime {
    /// Sends every geometry request of `layout` and the keyboard mode `keyboard` for the new layer `proxy`.
    fn new(proxy: ZwlrLayerSurfaceV1, layout: ProtocolLayout, keyboard: KeyboardMode) -> Self {
        apply(&proxy, None, layout);
        apply_keyboard(&proxy, keyboard);
        Self { proxy }
    }

    /// Sends `keyboard`, the keyboard mode set by the runtime for the panel.
    pub fn send_keyboard(&self, keyboard: KeyboardMode) {
        apply_keyboard(&self.proxy, keyboard);
    }
}

/// Makes the layer-shell requests for the state of a layer surface: the geometry and the acknowledgement of a
/// configure event.
pub(super) struct LayerControl {
    proxy: ZwlrLayerSurfaceV1,
    /// The layout sent to the compositor last.
    layout: ProtocolLayout,
    /// The configure events received and not forgotten yet, oldest first, with the serial of each.
    configures: VecDeque<(ConfigureToken, u32)>,
}

/// A configure event of a layer with `serial`. The frame whose surface state contains `token` acknowledges it.
#[derive(Clone, Copy)]
pub struct LayerConfigure {
    pub token: ConfigureToken,
    pub serial: u32,
}

/// A layer-shell request for the state of a layer surface.
#[derive(Clone, Copy)]
pub(super) enum LayerRequest {
    /// Sets the parts of the geometry that differ from the layout sent last.
    Layout(ProtocolLayout),
    AckConfigure(u32),
}

impl LayerControl {
    /// The control of the layer `proxy`, whose role sent `layout` already.
    pub fn new(proxy: ZwlrLayerSurfaceV1, layout: ProtocolLayout) -> Self {
        Self {
            proxy,
            layout,
            configures: VecDeque::new(),
        }
    }

    /// Records `configure` until the frame that applies it.
    pub fn receive_configure(&mut self, configure: LayerConfigure) {
        self.configures
            .push_back((configure.token, configure.serial));
    }

    /// The request for `change`, or `None` for a part of the state that the layer role does not set.
    pub fn request(
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

    pub fn send(&mut self, request: LayerRequest) {
        match request {
            LayerRequest::Layout(next) => {
                apply(&self.proxy, Some(self.layout), next);
                self.layout = next;
            }
            LayerRequest::AckConfigure(serial) => self.proxy.ack_configure(serial),
        }
    }

    /// Forgets the configure events received before the event of `token`, because the surface state applies only the
    /// last configure event, and returns the serial of `token`. Returns `None` when the event was forgotten already.
    fn forget_until(&mut self, token: ConfigureToken) -> Option<u32> {
        while let Some(&(oldest, serial)) = self.configures.front() {
            if oldest == token {
                return Some(serial);
            }
            let _superseded = self.configures.pop_front();
        }
        None
    }
}

impl Surface {
    /// Sends `keyboard`, the keyboard mode set by the runtime for the panel on this surface, `id`.
    fn send_keyboard(&self, id: SurfaceId, keyboard: KeyboardMode) -> Result<(), LayerError> {
        let NativeRole::Layer(layer) = &self.role else {
            return Err(LayerError::PanelWithoutLayer(id));
        };
        layer.send_keyboard(keyboard);
        Ok(())
    }
}

/// The state of a layer surface with `geometry`, sent to the compositor with the role of the surface.
pub(super) fn role_state(geometry: PanelGeometry) -> SurfaceState {
    let mut state = SurfaceState::default();
    state.apply(SurfaceChange::Panel(geometry));
    state
}

/// Gives `wl` the role of a panel through its layer object `proxy`: sends `layout` and `keyboard`, and commits `wl`,
/// so that the compositor sends the first configure event. Returns the role with the control of its state.
pub(super) fn panel_role(
    wl: &wl_surface::WlSurface,
    proxy: ZwlrLayerSurfaceV1,
    layout: ProtocolLayout,
    keyboard: KeyboardMode,
) -> (NativeRole, RoleControl) {
    let control = LayerControl::new(proxy.clone(), layout);
    let layer = Runtime::new(proxy, layout, keyboard);
    wl.commit();
    (
        NativeRole::Layer(Box::new(layer)),
        RoleControl::Layer(control),
    )
}

impl Translator {
    /// Creates the layer object of `surface` for `panel`, whose native surface is `wl`, on `output` or on the output
    /// chosen by the compositor.
    pub(super) fn layer_surface(
        &self,
        wl: &wl_surface::WlSurface,
        surface: SurfaceId,
        output: Option<&wl_output::WlOutput>,
        panel: &PanelRole,
    ) -> Result<ZwlrLayerSurfaceV1, LayerError> {
        let shell = (self.protocols.layers.as_ref()).ok_or(LayerError::NoLayerShell)?;
        let (level, namespace) = (panel.level, &panel.namespace);
        Ok(create(
            shell, wl, output, level, namespace, &self.qh, surface,
        ))
    }

    /// Executes a request of the runtime for a panel. A request for a panel whose native surface is gone changes
    /// nothing.
    pub(crate) fn execute_panel(&self, request: PanelRequest) -> Result<(), LayerError> {
        match request {
            PanelRequest::SetKeyboard(surface, keyboard) => (self.surfaces.live(surface))
                .map_or(Ok(()), |native| native.send_keyboard(surface, keyboard)),
        }
    }
}

/// Translates a configure event of a layer with `serial`, `width` and `height` into the message that records the
/// serial in the control of the layer and the surface event with the same token. The frame whose surface state
/// contains the token acknowledges the serial. The compositor sends zero for an axis that it leaves to the layer.
pub(super) fn translate_configure(
    serial: u32,
    width: u32,
    height: u32,
) -> (LayerConfigure, SurfaceEvent) {
    let token = ConfigureToken::default();
    let event = configure_event(NonZero::new(width), NonZero::new(height), Some(token));
    (LayerConfigure { token, serial }, event)
}

fn create(
    shell: &ZwlrLayerShellV1,
    wl: &wl_surface::WlSurface,
    output: Option<&wl_output::WlOutput>,
    level: LayerLevel,
    namespace: &str,
    qh: &QueueHandle<Translator>,
    id: SurfaceId,
) -> ZwlrLayerSurfaceV1 {
    let layer = match level {
        LayerLevel::Background => zwlr_layer_shell_v1::Layer::Background,
        LayerLevel::Bottom => zwlr_layer_shell_v1::Layer::Bottom,
        LayerLevel::Top => zwlr_layer_shell_v1::Layer::Top,
        LayerLevel::Overlay => zwlr_layer_shell_v1::Layer::Overlay,
    };
    shell.get_layer_surface(wl, output, layer, namespace.to_owned(), qh, id)
}

/// Sends `keyboard`. Version 4 of layer-shell adds on-demand keyboard interactivity. An older compositor gives an
/// interactive layer the keyboard exclusively.
fn apply_keyboard(proxy: &ZwlrLayerSurfaceV1, keyboard: KeyboardMode) {
    use zwlr_layer_surface_v1::KeyboardInteractivity;
    proxy.set_keyboard_interactivity(match keyboard {
        KeyboardMode::Off => KeyboardInteractivity::None,
        KeyboardMode::OnDemand if proxy.version() >= 4 => KeyboardInteractivity::OnDemand,
        KeyboardMode::OnDemand => KeyboardInteractivity::Exclusive,
    });
}

mod error;
mod protocol_layout;

#[cfg(test)]
mod focus_wire;
#[cfg(test)]
mod focus_wire_tests;
#[cfg(test)]
mod state_wire_tests;
#[cfg(test)]
pub(crate) mod test_error;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod wire_tests;
