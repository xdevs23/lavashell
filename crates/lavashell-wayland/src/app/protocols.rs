// SPDX-License-Identifier: GPL-3.0-only
use super::{
    CompositorHandler, Connection, Density, Dispatch, ExtBackgroundEffectManagerV1,
    ExtBackgroundEffectSurfaceV1, NativeWindow, OutputHandler, OutputState, Platform,
    ProvidesRegistryState, QueueHandle, RegistryState, SeatState, Shm, ShmHandler, SurfaceEvent,
    SurfaceId, WaylandSurface, WindowConfigure, WindowHandler, WpFractionalScaleManagerV1,
    WpFractionalScaleV1, WpViewport, WpViewporter, ZwlrLayerShellV1, ZwlrLayerSurfaceV1,
    configure_event, delegate_noop, sctk, wl_output, wl_surface, wp_fractional_scale_v1,
    zwlr_layer_surface_v1,
};
use std::num::NonZero;

/// wp-fractional-scale-v1 sends the preferred scale as the numerator of a fraction over 120.
const FRACTIONAL_SCALE_DENOMINATOR: u32 = 120;

impl Platform {
    fn refresh_fallback(&mut self) {
        if self.presentation.is_some() {
            return;
        }
        for (id, surface) in &mut self.surfaces {
            let interval = self.output_records.refreshes(*id).min();
            surface.clock.output_refresh(interval);
        }
    }
}

impl CompositorHandler for Platform {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        wl: &wl_surface::WlSurface,
        scale: i32,
    ) {
        if let Some(id) = self.find_surface(wl)
            && (self.surfaces.get(&id)).is_some_and(|surface| surface.fractional.is_none())
        {
            match Density::from_scale(f64::from(scale)) {
                Some(density) => self.surface_event(id, SurfaceEvent::DensityChanged(density)),
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

    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, wl: &wl_surface::WlSurface, _: u32) {
        if let Some(id) = self.find_surface(wl)
            && let Some(surface) = self.surfaces.get_mut(&id)
        {
            surface.frame_callback = None;
        }
    }

    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        wl: &wl_surface::WlSurface,
        output: &wl_output::WlOutput,
    ) {
        self.report_surface_entered(wl, output);
        self.refresh_fallback();
    }

    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        wl: &wl_surface::WlSurface,
        output: &wl_output::WlOutput,
    ) {
        self.report_surface_left(wl, output);
        self.refresh_fallback();
    }
}

impl WindowHandler for Platform {
    fn request_close(&mut self, _: &Connection, _: &QueueHandle<Self>, window: &NativeWindow) {
        if let Some(id) = self.find_surface(window.wl_surface()) {
            self.surface_event(id, SurfaceEvent::Closed);
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
            let event = configure_event(width.map(NonZero::get), height.map(NonZero::get));
            self.surface_event(id, event);
        }
    }
}

impl Dispatch<ZwlrLayerSurfaceV1, SurfaceId> for Platform {
    fn event(
        state: &mut Self,
        layer: &ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        id: &SurfaceId,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_layer_surface_v1::Event::Configure {
                serial,
                width,
                height,
            } => {
                layer.ack_configure(serial);
                let event = state.layer_configure(*id, width, height);
                state.surface_event(*id, event);
            }
            zwlr_layer_surface_v1::Event::Closed => state.surface_event(*id, SurfaceEvent::Closed),
            _ => {}
        }
    }
}

impl Dispatch<WpFractionalScaleV1, SurfaceId> for Platform {
    fn event(
        state: &mut Self,
        _: &WpFractionalScaleV1,
        event: wp_fractional_scale_v1::Event,
        id: &SurfaceId,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wp_fractional_scale_v1::Event::PreferredScale { scale } = event {
            match Density::from_ratio(f64::from(scale), f64::from(FRACTIONAL_SCALE_DENOMINATOR)) {
                Ok(density) => state.surface_event(*id, SurfaceEvent::DensityChanged(density)),
                Err(error) => log::warn!("the fractional scale {scale} is not usable: {error}"),
            }
        }
    }
}

impl OutputHandler for Platform {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }

    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, output: wl_output::WlOutput) {
        self.report_output(&output);
        self.layers_output_changed(&output);
        self.lock_output_added(&output);
    }

    fn update_output(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        self.report_output(&output);
        self.layers_output_changed(&output);
        self.refresh_fallback();
    }

    fn output_destroyed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        self.layers_output_removed(&output);
        self.lock_output_removed(&output);
        self.remove_output(&output);
        self.refresh_fallback();
    }
}

impl ShmHandler for Platform {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for Platform {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }

    sctk::registry_handlers![OutputState, SeatState];
}

sctk::delegate_registry!(Platform);

sctk::delegate_dispatch2!(Platform);

delegate_noop!(Platform:ignore ZwlrLayerShellV1);

delegate_noop!(Platform:ignore WpViewporter);

delegate_noop!(Platform:ignore WpViewport);

delegate_noop!(Platform:ignore WpFractionalScaleManagerV1);

delegate_noop!(Platform:ignore ExtBackgroundEffectManagerV1);

delegate_noop!(Platform:ignore ExtBackgroundEffectSurfaceV1);
