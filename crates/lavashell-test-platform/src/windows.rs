// SPDX-License-Identifier: GPL-3.0-only
use crate::capture::{CaptureMode, ShownFrame};
use crate::{Error, FrameCapture};
use lavashell_actor::Sender;
use lavashell_core::surface::SurfaceId;
use lavashell_core::units::Density;
use lavashell_core::window::{WindowCapabilities, WindowState};
use lavashell_platform::{
    HeadlessRendererBackend, Reports, Role, SurfaceAttachment, SurfaceEvent, SurfaceSpec,
    WindowControlRequest,
};
use std::collections::BTreeMap;
use std::marker::PhantomData;

type TestReports<B> = Reports<FrameCapture, B>;

pub(crate) struct WindowRecording {
    pub app_ids: BTreeMap<SurfaceId, String>,
    pub requests: Vec<WindowControlRequest>,
}

pub(crate) struct Windows<B> {
    density: Density,
    capture: CaptureMode,
    shown_by_controls: Sender<ShownFrame>,
    open: Vec<(SurfaceId, Role)>,
    app_ids: BTreeMap<SurfaceId, String>,
    states: BTreeMap<SurfaceId, WindowState>,
    recorded_requests: Vec<WindowControlRequest>,
    backend: PhantomData<B>,
}

impl<B: HeadlessRendererBackend> Windows<B> {
    pub(crate) fn new(density: Density, shown_by_controls: Sender<ShownFrame>) -> Self {
        Self {
            density,
            capture: CaptureMode::WithoutPixels,
            shown_by_controls,
            open: Vec::new(),
            app_ids: BTreeMap::new(),
            states: BTreeMap::new(),
            recorded_requests: Vec::new(),
            backend: PhantomData,
        }
    }

    pub(crate) fn capture(&mut self, reports: &TestReports<B>, mode: CaptureMode) {
        self.capture = mode;
        for &(surface, _) in &self.open {
            reports.attach(SurfaceAttachment::Control {
                surface,
                message: mode,
            });
        }
    }

    pub(crate) fn create(
        &mut self,
        reports: &TestReports<B>,
        surface: SurfaceId,
        spec: SurfaceSpec,
    ) {
        let control = FrameCapture::new(surface, self.capture, self.shown_by_controls.clone());
        let attach = spec.attach(surface, B::create_surface(), control);
        reports.attach(SurfaceAttachment::Attach(Box::new(attach)));
        reports.report((surface, SurfaceEvent::DensityChanged(self.density)));
        let window = opened_window_state(&spec.role);
        reports.report((
            surface,
            SurfaceEvent::Configured {
                width: Some(spec.size.width),
                height: Some(spec.size.height),
                token: None,
                window,
            },
        ));
        if let Some(state) = window {
            let _created_once = self.states.insert(surface, state);
        }
        if let Role::Window(window) = &spec.role {
            let _created_once = self.app_ids.insert(surface, window.app_id.clone());
        }
        self.open.push((surface, spec.role));
    }

    pub(crate) fn destroy(&mut self, reports: &TestReports<B>, surface: SurfaceId) {
        self.open.retain(|&(window, _)| window != surface);
        let _state_of_destroyed_window = self.states.remove(&surface);
        reports.attach(SurfaceAttachment::Detach(surface));
    }

    pub(crate) fn control(
        &mut self,
        reports: &TestReports<B>,
        request: WindowControlRequest,
    ) -> Result<(), Error> {
        match request {
            WindowControlRequest::SetMaximized { surface, maximized } => {
                self.reconfigure(reports, surface, |state| WindowState { maximized, ..state })?;
            }
            WindowControlRequest::SetFullscreen {
                surface,
                fullscreen,
            } => self.reconfigure(reports, surface, |state| WindowState {
                fullscreen,
                ..state
            })?,
            WindowControlRequest::BeginMove { .. }
            | WindowControlRequest::BeginResize { .. }
            | WindowControlRequest::Minimize { .. }
            | WindowControlRequest::ShowMenu { .. } => self.recorded_requests.push(request),
        }
        Ok(())
    }

    fn reconfigure(
        &mut self,
        reports: &TestReports<B>,
        surface: SurfaceId,
        change: impl FnOnce(WindowState) -> WindowState,
    ) -> Result<(), Error> {
        let state = (self.states.get_mut(&surface)).ok_or(Error::NotAWindow { surface })?;
        *state = change(*state);
        let configure = SurfaceEvent::Configured {
            width: None,
            height: None,
            token: None,
            window: Some(*state),
        };
        reports.report((surface, configure));
        Ok(())
    }

    pub(crate) fn into_recorded(self) -> WindowRecording {
        WindowRecording {
            app_ids: self.app_ids,
            requests: self.recorded_requests,
        }
    }

    pub(crate) fn surfaces(&self) -> impl Iterator<Item = SurfaceId> {
        self.open.iter().map(|&(surface, _)| surface)
    }

    pub(crate) fn find(&self, app_id: &str) -> Result<SurfaceId, Error> {
        (self.open.iter())
            .find(|(_, role)| matches!(role, Role::Window(window) if window.app_id == app_id))
            .map(|&(surface, _)| surface)
            .ok_or_else(|| Error::NoWindow {
                app_id: app_id.to_owned(),
            })
    }
}

fn opened_window_state(role: &Role) -> Option<WindowState> {
    let Role::Window(window) = role else {
        return None;
    };
    Some(WindowState {
        fullscreen: window.fullscreen,
        capabilities: WindowCapabilities::all(),
        ..WindowState::default()
    })
}

#[cfg(test)]
mod tests;
