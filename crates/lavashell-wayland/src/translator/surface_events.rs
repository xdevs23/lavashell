// SPDX-License-Identifier: GPL-3.0-only
mod window_state;

use super::Translator;
use crate::native::{configure_event, translate_configure};
use crate::protocol_serial::ProtocolSerial;
use crate::sctk;
use lavashell_core::surface::SurfaceId;
use lavashell_core::units::Density;
use lavashell_platform::{OutputEvent, SurfaceAttachment, SurfaceEvent, TimingEvent};
use sctk::compositor::CompositorHandler;
use sctk::reexports::client::protocol::wl_callback::{self, WlCallback};
use sctk::reexports::client::protocol::{wl_output, wl_surface};
use sctk::reexports::client::{Connection, Dispatch, QueueHandle};
use sctk::reexports::protocols::wp::fractional_scale::v1::client::wp_fractional_scale_v1::{
    self, WpFractionalScaleV1,
};
use sctk::reexports::protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::{
    self, ZwlrLayerSurfaceV1,
};
use sctk::shell::WaylandSurface;
use sctk::shell::xdg::window::{Window as NativeWindow, WindowConfigure, WindowHandler};
use window_state::read_window_state;

// https://gitlab.freedesktop.org/wayland/wayland-protocols/-/blob/main/staging/fractional-scale/fractional-scale-v1.xml
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
                .is_some_and(|surface| !surface.follows_fractional_scale())
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
        if let Some((surface, output)) = self.surface_and_output(wl, output) {
            self.links.report(OutputEvent::Entered(output, surface));
        }
    }

    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        wl: &wl_surface::WlSurface,
        output: &wl_output::WlOutput,
    ) {
        if let Some((surface, output)) = self.surface_and_output(wl, output) {
            self.links.report(OutputEvent::Left(output, surface));
        }
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
            let state = Some(read_window_state(&configure));
            self.links
                .report((id, configure_event(width, height, None, state)));
        }
    }
}

impl Dispatch<ZwlrLayerSurfaceV1, SurfaceId> for Translator {
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
                let (message, event) =
                    translate_configure(ProtocolSerial::new(serial), width, height);
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
