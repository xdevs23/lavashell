// SPDX-License-Identifier: GPL-3.0-only
use super::{
    CompositorHandler, Connection, Density, Dispatch, Duration, ExtBackgroundEffectManagerV1,
    ExtBackgroundEffectSurfaceV1, NativeWindow, OutputHandler, OutputState, Platform,
    ProvidesRegistryState, Proxy, QueueHandle, RegistryState, SeatState, Shm, ShmHandler,
    WaylandSurface, WindowConfigure, WindowHandler, WpFractionalScaleManagerV1,
    WpFractionalScaleV1, WpViewport, WpViewporter, ZwlrLayerShellV1, ZwlrLayerSurfaceV1,
    delegate_noop, sctk, wl_output, wl_surface, wp_fractional_scale_v1, zwlr_layer_surface_v1,
};

/// wp-fractional-scale-v1 sends the preferred scale as the numerator of a fraction over 120.
const FRACTIONAL_SCALE_DENOMINATOR: u32 = 120;
/// `wl_output` sends the refresh rate of a mode in mHz.
const MILLIHERTZ_PER_HERTZ: f64 = 1000.0;

impl Platform {
    fn refresh_fallback(&mut self) {
        if self.presentation.is_some() {
            return;
        }
        for surface in &mut self.surfaces {
            let interval = surface
                .wl
                .data::<sctk::compositor::SurfaceData<()>>()
                .into_iter()
                .flat_map(smithay_client_toolkit::compositor::SurfaceData::outputs)
                .filter_map(|output| self.outputs.info(&output))
                .flat_map(|info| info.modes)
                .filter(|mode| mode.current && mode.refresh_rate > 0)
                .map(|mode| {
                    Duration::from_secs_f64(MILLIHERTZ_PER_HERTZ / f64::from(mode.refresh_rate))
                })
                .min();
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
        if let Some(index) = self
            .index(wl)
            .filter(|index| self.surfaces[*index].fractional.is_none())
        {
            match Density::from_scale(f64::from(scale)) {
                Some(density) => {
                    let surface = &mut self.surfaces[index];
                    surface.density = density;
                    surface.view.invalidate();
                }
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
        if let Some(index) = self.index(wl) {
            self.surfaces[index].frame_callback = None;
        }
    }

    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
        self.refresh_fallback();
    }

    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
        self.refresh_fallback();
    }
}

impl WindowHandler for Platform {
    fn request_close(&mut self, _: &Connection, _: &QueueHandle<Self>, window: &NativeWindow) {
        if let Some(index) = self.index(window.wl_surface()) {
            self.surfaces[index].lifecycle.close();
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
        if let Some(index) = self.index(window.wl_surface()) {
            let surface = &mut self.surfaces[index];
            surface.size = (
                configure
                    .new_size
                    .0
                    .map_or(surface.size.0, std::num::NonZero::get),
                configure
                    .new_size
                    .1
                    .map_or(surface.size.1, std::num::NonZero::get),
            );
            surface.lifecycle.configure();
            surface.view.invalidate();
        }
    }
}

impl Dispatch<ZwlrLayerSurfaceV1, usize> for Platform {
    fn event(
        state: &mut Self,
        layer: &ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        id: &usize,
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
                let surface = &mut state.surfaces[*id];
                surface.size = (
                    if width == 0 { surface.size.0 } else { width },
                    if height == 0 { surface.size.1 } else { height },
                );
                surface.lifecycle.configure();
                surface.view.invalidate();
            }
            zwlr_layer_surface_v1::Event::Closed => state.surfaces[*id].lifecycle.close(),
            _ => {}
        }
    }
}

impl Dispatch<WpFractionalScaleV1, usize> for Platform {
    fn event(
        state: &mut Self,
        _: &WpFractionalScaleV1,
        event: wp_fractional_scale_v1::Event,
        id: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wp_fractional_scale_v1::Event::PreferredScale { scale } = event {
            match Density::from_ratio(f64::from(scale), f64::from(FRACTIONAL_SCALE_DENOMINATOR)) {
                Ok(density) => {
                    state.surfaces[*id].density = density;
                    state.surfaces[*id].view.invalidate();
                }
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
        self.layers_output_changed(output.clone());
        self.lock_output_added(&output);
    }

    fn update_output(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        self.layers_output_changed(output);
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
