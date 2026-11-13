// SPDX-License-Identifier: GPL-3.0-only
use super::{configure_event, layer};
use crate::translator::Translator;
use lavashell_core::surface::SurfaceId;
use lavashell_core::units::Density;
use lavashell_platform::{SurfaceAttachment, SurfaceEvent, TimingEvent};
use sctk::compositor::CompositorHandler;
use sctk::output::{OutputHandler, OutputState};
use sctk::reexports::client::protocol::wl_callback::{self, WlCallback};
use sctk::reexports::client::protocol::{wl_output, wl_region::WlRegion, wl_surface};
use sctk::reexports::client::{Connection, Dispatch, QueueHandle, delegate_noop};
use sctk::reexports::protocols::ext::background_effect::v1::client::{
    ext_background_effect_manager_v1::ExtBackgroundEffectManagerV1,
    ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1,
};
use sctk::reexports::protocols::wp::{
    fractional_scale::v1::client::{
        wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1,
        wp_fractional_scale_v1::{self, WpFractionalScaleV1},
    },
    viewporter::client::{wp_viewport::WpViewport, wp_viewporter::WpViewporter},
};
use sctk::reexports::protocols_wlr::layer_shell::v1::client::{
    zwlr_layer_shell_v1::ZwlrLayerShellV1,
    zwlr_layer_surface_v1::{self, ZwlrLayerSurfaceV1},
};
use sctk::registry::{ProvidesRegistryState, RegistryState};
use sctk::seat::SeatState;
use sctk::shell::WaylandSurface;
use sctk::shell::xdg::window::{Window as NativeWindow, WindowConfigure, WindowHandler};
use sctk::shm::{Shm, ShmHandler};
use smithay_client_toolkit as sctk;

/// wp-fractional-scale-v1 sends the preferred scale as the numerator of a fraction over 120.
const FRACTIONAL_SCALE_DENOMINATOR: u32 = 120;

impl Dispatch<WlCallback, SurfaceId> for Translator {
    fn event(
        state: &mut Self,
        _: &WlCallback,
        event: wl_callback::Event,
        id: &SurfaceId,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_callback::Event::Done { .. } = event
            && state.surfaces.is_live(*id)
        {
            state.links.time(TimingEvent::DisplayReady(*id));
        }
    }
}

impl CompositorHandler for Translator {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        wl: &wl_surface::WlSurface,
        scale: i32,
    ) {
        if let Some(id) = self.find_surface(wl)
            && self
                .surfaces
                .live(id)
                .is_some_and(|surface| surface.fractional.is_none())
        {
            match Density::from_scale(f64::from(scale)) {
                Some(density) => self
                    .links
                    .report((id, SurfaceEvent::DensityChanged(density))),
                None => log::warn!("the scale {scale} of a surface is not above zero"),
            }
        }
    }

    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }

    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {}

    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        wl: &wl_surface::WlSurface,
        output: &wl_output::WlOutput,
    ) {
        self.report_surface_entered(wl, output);
    }

    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        wl: &wl_surface::WlSurface,
        output: &wl_output::WlOutput,
    ) {
        self.report_surface_left(wl, output);
    }
}

impl WindowHandler for Translator {
    fn request_close(&mut self, _: &Connection, _: &QueueHandle<Self>, window: &NativeWindow) {
        if let Some(id) = self.find_surface(window.wl_surface()) {
            self.links.report((id, SurfaceEvent::Closed));
        }
    }

    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        window: &NativeWindow,
        configure: WindowConfigure,
        _: u32,
    ) {
        if let Some(id) = self.find_surface(window.wl_surface()) {
            let (width, height) = configure.new_size;
            self.links
                .report((id, configure_event(width, height, None)));
        }
    }
}

impl Dispatch<ZwlrLayerSurfaceV1, SurfaceId> for Translator {
    /// The control of the layer records the serial of a configure event. The first commit whose surface state applies
    /// the event acknowledges it. The serial is sent to the renderer before the token is sent to the view, so every
    /// frame that contains the token follows the serial. A destroyed surface reports nothing.
    fn event(
        state: &mut Self,
        _: &ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        id: &SurfaceId,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if !state.surfaces.is_live(*id) {
            return;
        }
        match event {
            zwlr_layer_surface_v1::Event::Configure {
                serial,
                width,
                height,
            } => {
                let (message, event) = layer::translate_configure(serial, width, height);
                state.links.attach(SurfaceAttachment::Control {
                    surface: *id,
                    message,
                });
                state.links.report((*id, event));
            }
            zwlr_layer_surface_v1::Event::Closed => state.links.report((*id, SurfaceEvent::Closed)),
            _ => {}
        }
    }
}

impl Dispatch<WpFractionalScaleV1, SurfaceId> for Translator {
    fn event(
        state: &mut Self,
        _: &WpFractionalScaleV1,
        event: wp_fractional_scale_v1::Event,
        id: &SurfaceId,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wp_fractional_scale_v1::Event::PreferredScale { scale } = event
            && state.surfaces.is_live(*id)
        {
            match Density::from_ratio(f64::from(scale), f64::from(FRACTIONAL_SCALE_DENOMINATOR)) {
                Ok(density) => state
                    .links
                    .report((*id, SurfaceEvent::DensityChanged(density))),
                Err(error) => log::warn!("the fractional scale {scale} is not usable: {error}"),
            }
        }
    }
}

impl OutputHandler for Translator {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }

    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, output: wl_output::WlOutput) {
        self.add_output(&output);
    }

    fn update_output(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        self.report_output(&output);
    }

    fn output_destroyed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        self.remove_output(&output);
    }
}

impl ShmHandler for Translator {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for Translator {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }

    sctk::registry_handlers![OutputState, SeatState];
}

sctk::delegate_registry!(Translator);

sctk::delegate_dispatch2!(Translator);

delegate_noop!(Translator:ignore ZwlrLayerShellV1);

delegate_noop!(Translator:ignore WpViewporter);

delegate_noop!(Translator:ignore WpViewport);

delegate_noop!(Translator:ignore WlRegion);

delegate_noop!(Translator:ignore WpFractionalScaleManagerV1);

delegate_noop!(Translator:ignore ExtBackgroundEffectManagerV1);

delegate_noop!(Translator:ignore ExtBackgroundEffectSurfaceV1);
