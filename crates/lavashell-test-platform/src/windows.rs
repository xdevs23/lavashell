// SPDX-License-Identifier: GPL-3.0-only
use crate::capture::{CaptureMode, ShownFrame};
use crate::{Error, FrameCapture};
use lavashell_actor::Sender;
use lavashell_core::surface::SurfaceId;
use lavashell_core::units::Density;
use lavashell_core::window::{WindowCapabilities, WindowState};
use lavashell_platform::{Reports, Role, SurfaceAttachment, SurfaceEvent, SurfaceSpec};
use lavashell_renderer::NoStreams;
use lavashell_vulkan::HeadlessWindow;
use std::collections::BTreeMap;
use std::sync::Arc;

type TestReports = Reports<FrameCapture, NoStreams>;

pub(crate) struct Windows {
    density: Density,
    capture: CaptureMode,
    shown_by_controls: Sender<ShownFrame>,
    open: Vec<(SurfaceId, Role)>,
    app_ids: BTreeMap<SurfaceId, String>,
}

impl Windows {
    pub(crate) fn new(density: Density, shown_by_controls: Sender<ShownFrame>) -> Self {
        Self {
            density,
            capture: CaptureMode::WithoutPixels,
            shown_by_controls,
            open: Vec::new(),
            app_ids: BTreeMap::new(),
        }
    }

    pub(crate) fn capture(&mut self, reports: &TestReports, mode: CaptureMode) {
        self.capture = mode;
        for &(surface, _) in &self.open {
            reports.attach(SurfaceAttachment::Control {
                surface,
                message: mode,
            });
        }
    }

    pub(crate) fn create(&mut self, reports: &TestReports, surface: SurfaceId, spec: SurfaceSpec) {
        let control = FrameCapture::new(surface, self.capture, self.shown_by_controls.clone());
        let attach = spec.attach(surface, Arc::new(HeadlessWindow), control);
        reports.attach(SurfaceAttachment::Attach(Box::new(attach)));
        reports.report((surface, SurfaceEvent::DensityChanged(self.density)));
        reports.report((
            surface,
            SurfaceEvent::Configured {
                width: Some(spec.size.width),
                height: Some(spec.size.height),
                token: None,
                window: opened_window_state(&spec.role),
            },
        ));
        if let Role::Window(window) = &spec.role {
            let _created_once = self.app_ids.insert(surface, window.app_id.clone());
        }
        self.open.push((surface, spec.role));
    }

    pub(crate) fn destroy(&mut self, reports: &TestReports, surface: SurfaceId) {
        self.open.retain(|&(window, _)| window != surface);
        reports.attach(SurfaceAttachment::Detach(surface));
    }

    pub(crate) fn into_app_ids(self) -> BTreeMap<SurfaceId, String> {
        self.app_ids
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
