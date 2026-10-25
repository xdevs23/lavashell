// SPDX-License-Identifier: GPL-3.0-only
use super::{
    Connection, Dispatch, GlobalList, Platform, Proxy, QueueHandle, Role, delegate_noop, wl_surface,
};
use crate::focus_grab::protocol::{
    hyprland_focus_grab_manager_v1::HyprlandFocusGrabManagerV1 as Manager,
    hyprland_focus_grab_v1::{self, HyprlandFocusGrabV1 as Grab},
};

/// Whether a layer has its Hyprland focus grab.
enum GrabState {
    Released,
    Held(Grab),
    /// The compositor cleared the grab while the layer was still active. The layer can take a new grab only after it
    /// becomes inactive.
    Blocked,
}

/// What a layer can do with its focus grab in an update.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Readiness {
    /// The layer releases its grab, if it has one.
    Unavailable,
    /// The layer does not release a grab that it has, and does not take a new one.
    Holdable,
    /// The layer takes a grab when it has none.
    Acquirable,
}

pub(super) struct Runtime {
    configuration: crate::FocusGrab,
    state: GrabState,
}

impl Runtime {
    pub fn new(configuration: crate::FocusGrab) -> Self {
        Self {
            configuration,
            state: GrabState::Released,
        }
    }

    fn holds(&self, grab: &Grab) -> bool {
        matches!(&self.state, GrabState::Held(held) if held == grab)
    }

    /// Destroys the grab when the layer has one. A blocked state is left unchanged.
    fn release(&mut self) {
        if let GrabState::Held(proxy) = &self.state {
            if proxy.is_alive() {
                proxy.destroy();
            }
            self.state = GrabState::Released;
        }
    }

    pub(super) fn cleared(&mut self) {
        self.release();
        (self.configuration.cleared)();
        // `active` is read after the `cleared` callback, because the callback can make the layer inactive. A layer
        // that is still active is blocked until it becomes inactive, so a new grab cannot follow the cleared one at
        // once.
        if (self.configuration.active)() {
            self.state = GrabState::Blocked;
        }
    }

    fn update(
        &mut self,
        manager: Option<&Manager>,
        wl: &wl_surface::WlSurface,
        qh: &QueueHandle<Platform>,
        index: usize,
        readiness: Readiness,
    ) {
        if !(self.configuration.active)() {
            self.release();
            self.state = GrabState::Released;
        } else if readiness == Readiness::Unavailable {
            self.release();
        } else if readiness == Readiness::Acquirable
            && matches!(self.state, GrabState::Released)
            && let Some(manager) = manager
        {
            let proxy = manager.create_grab(qh, index);
            proxy.add_surface(wl);
            proxy.commit();
            self.state = GrabState::Held(proxy);
        }
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        self.release();
    }
}

pub(super) struct Capability(Option<Manager>);

impl Capability {
    pub fn bind(globals: &GlobalList, qh: &QueueHandle<Platform>) -> Self {
        Self(globals.bind(qh, 1..=1, ()).ok())
    }
}

impl super::layer::Runtime {
    pub(super) fn update_focus_grab(
        &mut self,
        manager: Option<&Manager>,
        wl: &wl_surface::WlSurface,
        qh: &QueueHandle<Platform>,
        index: usize,
        configured: bool,
        size: (u32, u32),
    ) {
        let readiness = if !(configured && self.keyboard_active() && self.input_enabled()) {
            Readiness::Unavailable
        } else if self.keyboard_enabled() && self.content_current(size) {
            Readiness::Acquirable
        } else {
            Readiness::Holdable
        };
        if let Some(grab) = &mut self.focus {
            grab.update(manager, wl, qh, index, readiness);
        }
    }
}

impl Platform {
    pub(super) fn update_focus_grabs(&mut self) {
        for (index, surface) in self.surfaces.iter_mut().enumerate() {
            let configured = surface.lifecycle.is_configured();
            if let Some(Role::Layer(layer)) = surface.lifecycle.role_mut() {
                layer.update_focus_grab(
                    self.focus_grabs.0.as_ref(),
                    &surface.wl,
                    &self.qh,
                    index,
                    configured,
                    surface.size,
                );
            }
        }
    }
}

impl Dispatch<Grab, usize> for Platform {
    fn event(
        state: &mut Self,
        proxy: &Grab,
        event: hyprland_focus_grab_v1::Event,
        index: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let hyprland_focus_grab_v1::Event::Cleared = event
            && let Some(surface) = state.surfaces.get_mut(*index)
            && let Some(Role::Layer(layer)) = surface.lifecycle.role_mut()
            && let Some(grab) = &mut layer.focus
            && grab.holds(proxy)
        {
            grab.cleared();
        }
    }
}

delegate_noop!(Platform: ignore Manager);

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, rc::Rc};

    #[test]
    fn cleared_observes_release_before_a_replacement_without_an_intermediate_update() {
        let active = Rc::new(Cell::new(true));
        let observed = active.clone();
        let closed = active.clone();
        let mut grab = Runtime::new(crate::FocusGrab::new(
            move || observed.get(),
            move || closed.set(false),
        ));
        grab.cleared();
        active.set(true);
        assert!(matches!(grab.state, GrabState::Released));
        assert!((grab.configuration.active)());
        grab.cleared();
        assert!(!active.get());
    }

    #[test]
    fn a_cleared_owner_that_stays_active_cannot_reacquire() {
        let mut grab = Runtime::new(crate::FocusGrab::new(|| true, || {}));
        grab.cleared();
        assert!(matches!(grab.state, GrabState::Blocked));
    }
}
