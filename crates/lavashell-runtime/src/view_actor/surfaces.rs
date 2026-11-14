// SPDX-License-Identifier: GPL-3.0-only
use super::ViewActor;
use super::frames::ViewSurface;
use super::panels::{OPENING_LENGTH, opening_size};
use crate::layer_surface::LayerSurface;
use crate::panels::Panel;
use crate::scheduler::SchedulerInput;
use crate::session_lock::LockStep;
use crate::shape::WindowShape;
use crate::view_actor::ViewError;
use crate::window::{DragRegion, Window};
use lavashell_core::geometry::Size;
use lavashell_core::output::OutputRequest;
use lavashell_core::surface::SurfaceId;
use lavashell_core::units::Density;
use lavashell_core::view::View;
use lavashell_platform::{
    Appearance, LockRequest, OutputId, PanelRole, Platform, PlatformRequest, Role, SurfaceEvent,
    SurfaceRequest, SurfaceSpec,
};
use std::mem;

/// The record of a new surface, besides the native spec sent to the platform.
struct Opening {
    shape: Option<WindowShape>,
    drag_region: Option<DragRegion>,
    accessible_name: String,
    composition_order: i32,
}

impl<P: Platform> ViewActor<P> {
    /// Opens a window that shows `view`.
    pub(super) fn open_window(&mut self, window: Window, mut view: View) -> Result<(), ViewError> {
        let spec = window.spec(&mut view, &mut self.shaper)?;
        let opening = Opening {
            shape: window.shape,
            drag_region: window.drag_region,
            accessible_name: window.title,
            composition_order: 0,
        };
        let _window = self.open_surface(spec, view, opening)?;
        Ok(())
    }

    /// Opens a panel that shows `view` with `options`, on `output` or on the output chosen by the host, and returns
    /// the id of the surface of the panel.
    pub(super) fn open_panel(
        &mut self,
        options: LayerSurface,
        mut view: View,
        output: Option<OutputId>,
    ) -> Result<SurfaceId, ViewError> {
        let panel = Panel::new(options.panel, &mut view, &mut self.shaper)?;
        let geometry = panel.geometry();
        let spec = SurfaceSpec {
            size: opening_size(geometry),
            appearance: Appearance {
                transparent: true,
                output: options.output,
                hdr_metadata: options.hdr_metadata,
                background_blur: options.background_blur,
            },
            role: Role::Panel(PanelRole {
                output,
                level: options.level,
                namespace: options.namespace.clone(),
                geometry,
                keyboard: panel.keyboard_mode(),
            }),
        };
        let opening = Opening {
            shape: options.shape,
            drag_region: None,
            accessible_name: options.namespace,
            composition_order: options.composition_order,
        };
        let id = self.open_surface(spec, view, opening)?;
        self.panels.open(id, panel);
        if let Some(grab) = options.focus_grab {
            self.focus_grabs.open(id, grab)?;
        }
        Ok(id)
    }

    /// Opens the lock surface on `output`, while the session lock has no lock surface there.
    pub(super) fn open_lock_surface(&mut self, output: OutputId) -> Result<(), ViewError> {
        let Some(view) = (self.session_lock.as_ref()).and_then(|lock| lock.view_for(output)) else {
            return Ok(());
        };
        let spec = SurfaceSpec {
            size: Size::new(OPENING_LENGTH, OPENING_LENGTH),
            appearance: Appearance {
                transparent: false,
                output: OutputRequest::default(),
                hdr_metadata: None,
                background_blur: false,
            },
            role: Role::Lock { output },
        };
        let opening = Opening {
            shape: None,
            drag_region: None,
            accessible_name: String::new(),
            composition_order: 0,
        };
        let surface = self.open_surface(spec, view, opening)?;
        if let Some(lock) = &mut self.session_lock {
            lock.opened(output, surface);
        }
        Ok(())
    }

    /// Opens the record of a surface with `view`, and asks the platform for the native surface of `spec` under the id
    /// of that record. Returns the id of the surface.
    fn open_surface(
        &mut self,
        spec: SurfaceSpec,
        view: View,
        opening: Opening,
    ) -> Result<SurfaceId, ViewError> {
        let clock = self.clock.clone();
        view.motion().time_source(move || clock.now());
        let id = self.records.open(
            view,
            spec.size,
            Density::ONE,
            opening.drag_region,
            opening.accessible_name,
        )?;
        let surface = ViewSurface::new(opening.shape, opening.composition_order);
        let _replaced_surface = self.surfaces.insert(id, surface);
        self.schedule(SchedulerInput::Opened(id));
        self.request(PlatformRequest::Surface(SurfaceRequest::Create {
            surface: id,
            spec: Box::new(spec),
            gpu_budget: self.ui_budget,
        }))?;
        Ok(id)
    }

    /// Executes the requests that followed from the events of the host. A destroyed surface leaves the records of the
    /// views at once, and the platform releases the native surface once the renderer retired it.
    pub(super) fn execute_surface_requests(&mut self) -> Result<(), ViewError> {
        for request in mem::take(&mut self.surface_requests) {
            if let SurfaceRequest::Destroy(id) = request {
                self.forget_surface(id)?;
            }
            self.request(PlatformRequest::Surface(request))?;
        }
        Ok(())
    }

    fn forget_surface(&mut self, id: SurfaceId) -> Result<(), ViewError> {
        self.schedule(SchedulerInput::Closed(id));
        self.pacing.forget(id);
        self.transfers.forget_surface(id, &mut self.records);
        self.outputs.forget(id);
        self.input.forget_surface(id);
        self.panels.forget(id);
        if let Some(request) = self.focus_grabs.forget(id) {
            self.request(PlatformRequest::FocusGrab(request))?;
        }
        self.action_queue.forget_surface(id);
        let _closed_surface = self.surfaces.remove(&id);
        Ok(())
    }

    /// Runs the lock step of this turn. The unlock closes every surface of the run, and the release ends the run.
    pub(super) fn update_lock(&mut self) -> Result<(), ViewError> {
        let Some(lock) = &mut self.session_lock else {
            return Ok(());
        };
        match lock.step()? {
            LockStep::Running => {}
            LockStep::Unlock => {
                self.request(PlatformRequest::Lock(LockRequest::Unlock))?;
                for &surface in self.surfaces.keys() {
                    let closed = self.records.handle(surface, SurfaceEvent::Closed);
                    self.surface_requests.extend(closed);
                }
            }
            LockStep::Released => {
                self.session_lock = None;
                self.run_end.end();
            }
        }
        Ok(())
    }
}
