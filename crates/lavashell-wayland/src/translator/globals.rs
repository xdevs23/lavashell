// SPDX-License-Identifier: GPL-3.0-only
use super::Translator;
use crate::sctk;
use sctk::output::OutputState;
use sctk::reexports::client::delegate_noop;
use sctk::reexports::client::protocol::wl_region::WlRegion;
use sctk::reexports::protocols::ext::background_effect::v1::client::{
    ext_background_effect_manager_v1::ExtBackgroundEffectManagerV1,
    ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1,
};
use sctk::reexports::protocols::wp::{
    fractional_scale::v1::client::wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1,
    viewporter::client::{wp_viewport::WpViewport, wp_viewporter::WpViewporter},
};
use sctk::reexports::protocols_wlr::layer_shell::v1::client::zwlr_layer_shell_v1::ZwlrLayerShellV1;
use sctk::registry::{ProvidesRegistryState, RegistryState};
use sctk::seat::SeatState;
use sctk::shm::{Shm, ShmHandler};

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
