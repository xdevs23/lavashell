// SPDX-License-Identifier: GPL-3.0-only
use super::ViewActor;
use super::open_surfaces::Opening;
use super::panels::{OPENING_LENGTH, opening_size};
use crate::layer_surface::LayerSurface;
use crate::panels::Panel;
use crate::scheduler::SchedulerInput;
use crate::session_lock::LockStep;
use crate::view_actor::ViewError;
use crate::window::Window;
use lavashell_core::geometry::Size;
use lavashell_core::output::OutputRequest;
use lavashell_core::surface::SurfaceId;
use lavashell_core::view::View;
use lavashell_platform::{
    OutputId, PanelRole, Platform, PlatformRequest, Role, SessionLockRequest, SurfaceAppearance,
    SurfaceRequest, SurfaceSpec,
};

impl<P: Platform> ViewActor<P> {
    pub(super) fn open_window(&mut self, window: Window, mut view: View) -> Result<(), ViewError> {
        let spec = window.spec(&mut view, &mut self.shaper)?;
        let opening = Opening {
            shape: window.shape,
            accessible_name: Some(window.title),
            composition_order: 0,
        };
        let _opened_surface_is_not_needed = self.open_surface(spec, view, opening)?;
        Ok(())
    }

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
            appearance: SurfaceAppearance {
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
            accessible_name: Some(options.namespace),
            composition_order: options.composition_order,
        };
        let id = self.open_surface(spec, view, opening)?;
        self.surfaces.add_panel(id, panel, options.focus_grab)?;
        Ok(id)
    }

    pub(super) fn open_lock_surface(&mut self, output: OutputId) -> Result<(), ViewError> {
        let Some(view) = self.outputs.lock_view_for(output) else {
            return Ok(());
        };
        let spec = SurfaceSpec {
            size: Size::new(OPENING_LENGTH, OPENING_LENGTH),
            appearance: SurfaceAppearance {
                transparent: false,
                output: OutputRequest::default(),
                hdr_metadata: None,
                background_blur: false,
            },
            role: Role::Lock { output },
        };
        let opening = Opening {
            shape: None,
            accessible_name: None,
            composition_order: 0,
        };
        let surface = self.open_surface(spec, view, opening)?;
        self.outputs.lock_surface_opened(output, surface);
        Ok(())
    }

    fn open_surface(
        &mut self,
        spec: SurfaceSpec,
        view: View,
        opening: Opening,
    ) -> Result<SurfaceId, ViewError> {
        let clock = self.clock.clone();
        view.motion().time_source(move || clock.now());
        let id = self.surfaces.open(view, spec.size, opening)?;
        self.action_queue.open_surface(id);
        self.schedule(SchedulerInput::Opened(id));
        self.request(PlatformRequest::Surface(SurfaceRequest::Create {
            surface: id,
            spec: Box::new(spec),
        }))?;
        Ok(id)
    }

    pub(super) fn execute_surface_requests(&mut self) -> Result<(), ViewError> {
        for request in self.surfaces.take_requests() {
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
        self.transfers
            .forget_surface(id, self.surfaces.records_mut());
        self.outputs.forget_surface(id);
        self.input.forget_surface(id);
        if let Some(request) = self.surfaces.forget(id) {
            self.request(PlatformRequest::FocusGrab(request))?;
        }
        self.action_queue.forget_surface(id);
        Ok(())
    }

    pub(super) fn update_lock(&mut self) -> Result<(), ViewError> {
        let Some(step) = self.outputs.lock_step() else {
            return Ok(());
        };
        match step? {
            LockStep::Running => {}
            LockStep::Unlock => {
                self.request(PlatformRequest::Lock(SessionLockRequest::Unlock))?;
                self.surfaces.close_all();
            }
            LockStep::Released => {
                self.outputs.release_lock();
                self.run_end.end();
            }
        }
        Ok(())
    }
}
