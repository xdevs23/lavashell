// SPDX-License-Identifier: GPL-3.0-only
use crate::{Error, FrameInterval, fonts, without_failures};
use lavashell::core::scene::tree::{ApplyError, Change};
use lavashell::core::scene::{ChangeSink, SinkId};
use lavashell::core::surface::{SurfaceId, SurfaceIds};
use lavashell::prelude::*;
use std::{path::Path, time::Duration};

pub struct Capture {
    pub painter: Painter,
    gpu: VulkanRenderer,
    surface: SurfaceId,
}

impl Capture {
    pub fn new() -> Result<Self, Error> {
        let gpu = VulkanRenderer::headless().map_err(Error::Renderer)?;
        Ok(Self {
            painter: gpu.painter(&fonts()?).map_err(Error::Painter)?,
            gpu,
            surface: SurfaceIds::default()
                .next_id()
                .map_err(Error::NumberSurface)?,
        })
    }

    pub fn preferred_size(&mut self, view: &mut View) -> Result<Size, Error> {
        let unbounded = Proposal {
            width: None,
            height: None,
        };
        (view.size_that_fits(unbounded, self.painter.shaper())).map_err(Error::Measure)
    }

    fn size_or_preferred(&mut self, view: &mut View, size: Option<Size>) -> Result<Size, Error> {
        match size {
            Some(size) => Ok(size),
            None => self.preferred_size(view),
        }
    }

    pub fn screenshot(
        &mut self,
        view: &mut View,
        size: Option<Size>,
        path: impl AsRef<Path>,
    ) -> Result<(), Error> {
        let size = self.size_or_preferred(view, size)?;
        self.save(view, Viewport::new(size, Density::ONE), path)
    }

    pub fn save(
        &mut self,
        view: &mut View,
        viewport: Viewport,
        path: impl AsRef<Path>,
    ) -> Result<(), Error> {
        let mut builder = SceneBuilder::default();
        without_failures(view.frame_into(viewport, self.painter.shaper(), &mut builder))?;
        self.paint(builder.snapshot().at(view.motion().now()), viewport, path)
    }

    pub fn paint(
        &mut self,
        tree: Presented<'_>,
        viewport: Viewport,
        path: impl AsRef<Path>,
    ) -> Result<(), Error> {
        let pixels = (PixelScale::SQUARE.buffer_size(viewport)).map_err(Error::BufferSize)?;
        let mut surface = self.gpu.offscreen(pixels).map_err(Error::Offscreen)?;
        let _chained_canvas =
            (surface.canvas()).clear(lavashell::renderer::skia::Color::TRANSPARENT);
        (self.painter).paint_scene(surface.canvas(), self.surface, tree, viewport.density);
        let png = self.gpu.png(&mut surface).map_err(Error::EncodePng)?;
        write_capture(path, &png)
    }
}

pub fn write_capture(path: impl AsRef<Path>, png: &[u8]) -> Result<(), Error> {
    let path = path.as_ref();
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|source| Error::CreateDirectory {
            path: parent.to_owned(),
            source,
        })?;
    }
    std::fs::write(path, png).map_err(|source| Error::WriteCapture {
        path: path.to_owned(),
        source,
    })
}

pub struct OffscreenView {
    pub view: View,
    capture: Capture,
    scene: OffscreenScene,
    viewport: Viewport,
    time: Duration,
}

impl OffscreenView {
    pub fn new(mut view: View, size: Option<Size>, density: Density) -> Result<Self, Error> {
        let mut capture = Capture::new()?;
        let size = capture.size_or_preferred(&mut view, size)?;
        Ok(Self {
            view,
            capture,
            scene: OffscreenScene::default(),
            viewport: Viewport::new(size, density),
            time: Duration::ZERO,
        })
    }

    #[must_use]
    pub fn time(&self) -> Duration {
        self.time
    }

    #[must_use]
    pub fn viewport(&self) -> Viewport {
        self.viewport
    }

    pub fn at(&mut self, time: Duration) -> Result<(), Error> {
        while let Some(deadline) = (self.view.next_deadline()).filter(|deadline| *deadline < time) {
            self.time = deadline.max(self.time);
            self.frame()?;
        }
        self.time = time;
        self.frame()
    }

    pub fn frame(&mut self) -> Result<(), Error> {
        let timing = FrameTime {
            now: self.time,
            presentation: self.time,
            ..FrameTime::default()
        };
        let shaper = self.capture.painter.shaper();
        let frame = (self.view).frame_into_at(self.viewport, shaper, timing, &mut self.scene);
        without_failures(frame)
    }

    pub fn next_deadline(&self) -> Result<Duration, Error> {
        (self.view.next_deadline())
            .filter(|deadline| *deadline > self.time)
            .ok_or(Error::NothingMoves)
    }

    pub fn advance(&mut self) -> Result<(), Error> {
        self.time = self.next_deadline()?;
        self.frame()
    }

    pub fn settle(&mut self) -> Result<(), Error> {
        loop {
            self.frame()?;
            if self.view.motion().is_active() {
                self.time = self.next_deadline()?;
            } else if !self.view.needs_frame() {
                return Ok(());
            } else if !self.scene.last_frame_changed {
                return Err(Error::NoRest);
            }
        }
    }

    pub fn record_until_rest(
        &mut self,
        interval: FrameInterval,
        mut record: impl FnMut(&mut Self) -> Result<(), Error>,
    ) -> Result<(), Error> {
        self.frame()?;
        record(self)?;
        while self.view.motion().is_active() || self.view.needs_frame() {
            if self.view.motion().is_active() && self.view.next_deadline().is_none() {
                return Err(Error::NothingMoves);
            }
            self.at(self.time + interval.get())?;
            record(self)?;
        }
        Ok(())
    }

    pub fn save(&mut self, path: impl AsRef<Path>) -> Result<(), Error> {
        let snapshot = self.scene.builder.snapshot();
        (self.capture).paint(snapshot.at(self.time), self.viewport, path)
    }
}

#[derive(Default)]
struct OffscreenScene {
    builder: SceneBuilder,
    open_frame_changed: bool,
    last_frame_changed: bool,
}

impl ChangeSink for OffscreenScene {
    type Error = ApplyError;

    fn id(&self) -> SinkId {
        self.builder.id()
    }

    fn change(&mut self, change: Change) -> Result<(), ApplyError> {
        self.open_frame_changed = true;
        self.builder.change(change)
    }

    fn end_frame(&mut self) -> Result<(), ApplyError> {
        self.builder.end_frame()?;
        self.last_frame_changed = std::mem::take(&mut self.open_frame_changed);
        Ok(())
    }

    fn restart(&mut self) -> Result<(), ApplyError> {
        self.open_frame_changed = true;
        self.builder.restart()
    }
}

#[cfg(test)]
mod tests;
