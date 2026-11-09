// SPDX-License-Identifier: GPL-3.0-only
use super::control::{Control, RoleControl};
use super::creation::NativeSpec;
use super::{Platform, Role, Surface, configure_event, send_surface_state};
use crate::surface::LayerSurface;
use crate::{Error, LayerLayoutError};
use lavashell_actor::Closed;
use lavashell_core::panel::{LayerLevel, PanelGeometry};
use lavashell_core::surface::{ConfigureToken, SurfaceChange, SurfaceId, SurfaceState};
use lavashell_core::view::View;
use lavashell_platform::{Appearance, KeyboardMode, SurfaceEvent};
use lavashell_renderer::SurfaceContents;
use lavashell_runtime::{ContentState, Panel, Transaction};
use protocol_layout::{ProtocolLayout, apply, initial_size};
use sctk::reexports::client::protocol::{wl_output, wl_surface};
use sctk::reexports::client::{Proxy, QueueHandle};
use sctk::reexports::protocols_wlr::layer_shell::v1::client::{
    zwlr_layer_shell_v1::{self, ZwlrLayerShellV1},
    zwlr_layer_surface_v1::{self, ZwlrLayerSurfaceV1},
};
use smithay_client_toolkit as sctk;
use std::collections::VecDeque;

/// The layer-shell role of a surface.
pub(super) struct Runtime {
    pub composition_order: i32,
    pub proxy: ZwlrLayerSurfaceV1,
}

impl Runtime {
    /// Sends every geometry request of `layout` and the keyboard mode `keyboard` for the new layer `proxy`.
    fn new(
        proxy: ZwlrLayerSurfaceV1,
        options: &LayerSurface,
        layout: ProtocolLayout,
        keyboard: KeyboardMode,
    ) -> Self {
        apply(&proxy, None, layout);
        apply_keyboard(&proxy, keyboard);
        Self {
            composition_order: options.composition_order,
            proxy,
        }
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

    /// Records a configure event with `serial`, and returns the token for the surface state of the frame that applies
    /// the event.
    pub fn receive_configure(&mut self, serial: u32) -> ConfigureToken {
        let token = ConfigureToken::default();
        self.configures.push_back((token, serial));
        token
    }

    /// The width and the height sent to the compositor last, zero along an axis that fills.
    pub fn requested_size(&self) -> (u32, u32) {
        (self.layout.width, self.layout.height)
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
    /// Sends `keyboard`, the keyboard mode set by the runtime for the panel on this surface, `id`, once the mode
    /// changed.
    pub(super) fn send_keyboard(
        &self,
        id: SurfaceId,
        keyboard: Option<KeyboardMode>,
    ) -> Result<(), Error> {
        if let Some(keyboard) = keyboard {
            let Role::Layer(layer) = &self.role else {
                return Err(Error::PanelWithoutLayer(id));
            };
            layer.send_keyboard(keyboard);
        }
        Ok(())
    }
}

/// The state of a layer surface with `geometry`, sent to the compositor with the role of the surface.
fn role_state(geometry: PanelGeometry) -> SurfaceState {
    let mut state = SurfaceState::default();
    state.apply(SurfaceChange::Panel(geometry));
    state
}

impl NativeSpec {
    fn layer(
        options: &LayerSurface,
        view: View,
        geometry: PanelGeometry,
        layout: ProtocolLayout,
    ) -> Self {
        Self {
            view,
            size: initial_size(layout),
            appearance: Appearance {
                transparent: true,
                output: options.output,
                hdr_metadata: options.hdr_metadata,
                background_blur: options.background_blur,
            },
            shape: options.shape.clone(),
            drag_region: None,
            accessible_name: options.namespace.clone(),
            role_state: role_state(geometry),
        }
    }
}

impl Platform {
    /// Creates a layer surface that shows `view` with `options`, on `output` or on the output chosen by the
    /// compositor, and returns the id of the surface.
    pub(super) fn create_layer(
        &mut self,
        options: &LayerSurface,
        mut view: View,
        output: Option<&wl_output::WlOutput>,
    ) -> Result<SurfaceId, Error> {
        let panel = Panel::new(options.panel.clone(), &mut view, self.renderer.shaper())?;
        let geometry = panel.geometry();
        let layout = ProtocolLayout::new(geometry).map_err(Error::LayerLayout)?;
        let keyboard = panel.keyboard_mode();
        let spec = NativeSpec::layer(options, view, geometry, layout);
        let id = self.create_surface(spec, |platform, wl, id| {
            platform.layer_role(wl, output, options, layout, keyboard, id)
        })?;
        self.panels.open(id, panel);
        if let Some(grab) = &options.focus_grab {
            self.open_focus_grab(id, grab.clone());
        }
        Ok(id)
    }

    /// Gives `wl` the layer role on `output` and commits `wl`, so that the compositor sends the first configure event.
    /// Returns the role with the control of its state.
    fn layer_role(
        &self,
        wl: &wl_surface::WlSurface,
        output: Option<&wl_output::WlOutput>,
        options: &LayerSurface,
        layout: ProtocolLayout,
        keyboard: KeyboardMode,
        id: SurfaceId,
    ) -> Result<(Role, RoleControl), Error> {
        let shell = self.protocols.layers.as_ref().ok_or(Error::NoLayerShell)?;
        let proxy = create(shell, wl, output, options, &self.qh, id);
        let control = LayerControl::new(proxy.clone(), layout);
        let layer = Runtime::new(proxy, options, layout, keyboard);
        wl.commit();
        Ok((Role::Layer(Box::new(layer)), RoleControl::Layer(control)))
    }

    /// Sends the requests made by the runtime for each panel before a frame is rendered. While the compositor shows
    /// the current content of a panel, the renderer commits a changed geometry at once, without a frame.
    pub(super) fn update_layers(&mut self) -> Result<(), Error> {
        for (&id, surface) in &mut self.surfaces {
            if self.panels.get(id).is_none() {
                continue;
            }
            let Some(viewport) = self.surface_records.open_viewport(id) else {
                continue;
            };
            let Some(view) = self.surface_records.open_view_mut(id) else {
                continue;
            };
            let content = if surface.needs_visual_frame(view) {
                ContentState::Stale
            } else {
                match self
                    .renderer
                    .contents(id, viewport)
                    .map_err(Error::Render)?
                {
                    SurfaceContents::Current => ContentState::Current,
                    SurfaceContents::Outdated => ContentState::Stale,
                }
            };
            let update = self.panels.update_transaction(id, content);
            surface.send_keyboard(id, update.keyboard)?;
            match update.transaction {
                Transaction::Settled => {}
                Transaction::NeedsFrame => view.invalidate(),
                Transaction::Committed => {
                    let sink = self.scene.sink(id, None);
                    match send_surface_state(sink, &self.panels, id, []) {
                        Ok(true) => {}
                        Ok(false) => {
                            let built = self.scene.built(id);
                            (self.renderer.commit_state(id, &built.state))
                                .map_err(Error::CommitState)?;
                        }
                        Err(Closed) => self.scene_stopped = true,
                    }
                }
            }
        }
        Ok(())
    }
}

impl Platform {
    /// Records the configure event of layer `id` with `serial`, so that the first commit whose surface state applies
    /// the event acknowledges it. Returns the surface event for `width` and `height`, as [`configured_length`]
    /// describes for each axis.
    pub(super) fn layer_configure(
        &mut self,
        id: SurfaceId,
        serial: u32,
        width: u32,
        height: u32,
    ) -> SurfaceEvent {
        let layer = (self.renderer.control_mut(id)).and_then(Control::layer_mut);
        let (requested_width, requested_height) =
            (layer.as_deref()).map_or((0, 0), LayerControl::requested_size);
        if let Some(layer) = layer
            && self.surface_records.is_open(id)
        {
            let configure = SurfaceChange::Configure(layer.receive_configure(serial));
            if self.scene.sink(id, None).set_state(configure).is_err() {
                self.scene_stopped = true;
            }
        }
        configure_event(
            configured_length(width, requested_width),
            configured_length(height, requested_height),
        )
    }
}

/// The length of one axis in a configure event of a layer that last requested `requested` along it. The compositor
/// sends zero for an axis that it leaves to the layer, which then takes the requested length. A requested zero is an
/// axis that fills, so both zeros leave the length unknown.
fn configured_length(configured: u32, requested: u32) -> Option<u32> {
    [configured, requested]
        .into_iter()
        .find(|&length| length != 0)
}

fn create(
    shell: &ZwlrLayerShellV1,
    wl: &wl_surface::WlSurface,
    output: Option<&wl_output::WlOutput>,
    options: &LayerSurface,
    qh: &QueueHandle<Platform>,
    id: SurfaceId,
) -> ZwlrLayerSurfaceV1 {
    let layer = match options.level {
        LayerLevel::Background => zwlr_layer_shell_v1::Layer::Background,
        LayerLevel::Bottom => zwlr_layer_shell_v1::Layer::Bottom,
        LayerLevel::Top => zwlr_layer_shell_v1::Layer::Top,
        LayerLevel::Overlay => zwlr_layer_shell_v1::Layer::Overlay,
    };
    shell.get_layer_surface(wl, output, layer, options.namespace.clone(), qh, id)
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

mod protocol_layout;

#[cfg(test)]
mod focus_wire;
#[cfg(test)]
mod focus_wire_tests;
#[cfg(test)]
mod state_wire_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod wire_tests;
