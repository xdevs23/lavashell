// SPDX-License-Identifier: GPL-3.0-only
use super::{Platform, configure_event};
use lavashell_core::surface::SurfaceId;
use lavashell_core::units::Density;
use lavashell_platform::{SurfaceEvent, TimingEvent};
use lavashell_runtime::SchedulerInput;
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
use std::num::NonZero;

/// wp-fractional-scale-v1 sends the preferred scale as the numerator of a fraction over 120.
const FRACTIONAL_SCALE_DENOMINATOR: u32 = 120;

impl Platform {
    /// Reports to the scheduler the smallest refresh interval among the outputs that show each surface, when the host
    /// offers no `wp_presentation`.
    fn report_output_refreshes(&mut self) {
        if self.presentation.is_some() {
            return;
        }
        let refreshes: Vec<_> = (self.surfaces.keys())
            .map(|&surface| SchedulerInput::OutputRefresh {
                surface,
                refresh: self.output_records.refreshes(surface).min(),
            })
            .collect();
        for input in refreshes {
            self.schedule(input);
        }
    }
}

impl Dispatch<WlCallback, SurfaceId> for Platform {
    fn event(
        state: &mut Self,
        _: &WlCallback,
        event: wl_callback::Event,
        id: &SurfaceId,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_callback::Event::Done { .. } = event {
            state.schedule(SchedulerInput::Timing(TimingEvent::DisplayReady(*id)));
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

    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {}

    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        wl: &wl_surface::WlSurface,
        output: &wl_output::WlOutput,
    ) {
        self.report_surface_entered(wl, output);
        self.report_output_refreshes();
    }

    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        wl: &wl_surface::WlSurface,
        output: &wl_output::WlOutput,
    ) {
        self.report_surface_left(wl, output);
        self.report_output_refreshes();
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
        _: &ZwlrLayerSurfaceV1,
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
                let event = state.layer_configure(*id, serial, width, height);
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
        self.report_output_refreshes();
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
        self.report_output_refreshes();
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

delegate_noop!(Platform:ignore WlRegion);

delegate_noop!(Platform:ignore WpFractionalScaleManagerV1);

delegate_noop!(Platform:ignore ExtBackgroundEffectManagerV1);

delegate_noop!(Platform:ignore ExtBackgroundEffectSurfaceV1);
