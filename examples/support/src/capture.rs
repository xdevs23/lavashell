// SPDX-License-Identifier: GPL-3.0-only
use crate::{Error, FrameInterval, fonts, report_failures};
use lavashell::prelude::*;
use std::{path::Path, time::Duration};

pub struct Capture {
    pub painter: Painter,
    gpu: VulkanRenderer,
}

impl Capture {
    /// # Errors
    /// Returns an error when the fonts, the renderer or the painter cannot be created.
    pub fn new() -> Result<Self, Error> {
        let gpu = VulkanRenderer::headless().map_err(Error::Renderer)?;
        Ok(Self {
            painter: gpu.painter(&fonts()?).map_err(Error::Painter)?,
            gpu,
        })
    }

    /// The size of the content of `view`, measured with a proposal that leaves both the width and the height open.
    ///
    /// # Errors
    /// Returns an error when the view cannot be measured within that proposal.
    pub fn preferred_size(&mut self, view: &mut View) -> Result<Size, Error> {
        let unbounded = Proposal {
            width: None,
            height: None,
        };
        (view.size_that_fits(unbounded, self.painter.shaper())).map_err(Error::Measure)
    }

    /// Paints a frame of `view` in `viewport` into the PNG file at `path`, and reports each component that failed in
    /// that frame.
    ///
    /// # Errors
    /// Returns an error when the frame cannot be shown, rendered or written.
    pub fn save(
        &mut self,
        view: &mut View,
        viewport: Viewport,
        path: impl AsRef<Path>,
    ) -> Result<(), Error> {
        let mut tree = SceneTree::default();
        (view.show_frame(&mut tree, viewport, self.painter.shaper()))
            .map(report_failures)
            .map_err(Error::Frame)?;
        self.paint(tree.at(view.motion().now()), viewport, path)
    }

    /// Paints `tree` in `viewport` into the PNG file at `path`.
    ///
    /// # Errors
    /// Returns an error when the scene cannot be rendered or written.
    pub fn paint(
        &mut self,
        tree: Presented<'_>,
        viewport: Viewport,
        path: impl AsRef<Path>,
    ) -> Result<(), Error> {
        let path = path.as_ref();
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(|source| Error::CreateDirectory {
                path: parent.to_owned(),
                source,
            })?;
        }
        let Viewport { size, density } = viewport;
        let pixels = size.buffer_size(density).map_err(Error::BufferSize)?;
        let mut surface = self.gpu.offscreen(pixels).map_err(Error::Offscreen)?;
        let _chained_canvas =
            (surface.canvas()).clear(lavashell::renderer::skia::Color::TRANSPARENT);
        (self.painter).paint_scene(surface.canvas(), tree, density);
        let png = self.gpu.png(&mut surface).map_err(Error::EncodePng)?;
        std::fs::write(path, png).map_err(|source| Error::WriteCapture {
            path: path.to_owned(),
            source,
        })
    }
}

/// A view that is laid out in a viewport of a fixed size and rendered offscreen by Vulkan. The calling code sets the
/// time of each frame, so a capture does not depend on a display or a clock.
pub struct OffscreenView {
    pub view: View,
    capture: Capture,
    tree: SceneTree,
    viewport: Viewport,
    time: Duration,
}

impl OffscreenView {
    /// A view in a viewport of `size` with pixels of `density`. Without `size`, the viewport has the size of the
    /// content of `view`, measured with a proposal that leaves both the width and the height open.
    ///
    /// # Errors
    /// Returns an error when the capture cannot be created or the content of the view cannot be
    /// measured.
    pub fn new(mut view: View, size: Option<Size>, density: Density) -> Result<Self, Error> {
        let mut capture = Capture::new()?;
        let size = match size {
            Some(size) => size,
            None => capture.preferred_size(&mut view)?,
        };
        Ok(Self {
            view,
            capture,
            tree: SceneTree::default(),
            viewport: Viewport::new(size, density),
            time: Duration::ZERO,
        })
    }

    #[must_use]
    pub fn time(&self) -> Duration {
        self.time
    }

    /// The viewport of the captures: their size and the density of their pixels.
    #[must_use]
    pub fn viewport(&self) -> Viewport {
        self.viewport
    }

    /// Frames the view at each of its deadlines before `time`, the way a host that wakes at `View::next_deadline`
    /// does. Then frames the view at `time`.
    ///
    /// # Errors
    /// Returns an error when a frame cannot be shown.
    pub fn at(&mut self, time: Duration) -> Result<(), Error> {
        while let Some(deadline) = (self.view.next_deadline()).filter(|deadline| *deadline < time) {
            self.time = deadline.max(self.time);
            self.frame()?;
        }
        self.time = time;
        self.frame()
    }

    /// Frames the view at the time of the last frame, as a host does right after input.
    ///
    /// # Errors
    /// Returns an error when the frame cannot be shown.
    pub fn frame(&mut self) -> Result<(), Error> {
        let update = self.frame_update()?;
        self.show(update)
    }

    /// Frames the view at the time of the last frame, reports each component that failed in it and returns the
    /// changes to the scene. After a failed frame, the next update from the view contains the whole scene. The tree is
    /// emptied here so that this whole scene replaces the old one.
    fn frame_update(&mut self) -> Result<SceneUpdate, Error> {
        let timing = FrameTime {
            now: self.time,
            presentation: self.time,
            ..FrameTime::default()
        };
        let shaper = self.capture.painter.shaper();
        match (self.view).frame_at(self.viewport, shaper, timing) {
            Ok(frame) => {
                report_failures(frame.failures);
                Ok(frame.update)
            }
            Err(error) => {
                self.tree = SceneTree::default();
                Err(Error::Frame(error))
            }
        }
    }

    fn show(&mut self, update: SceneUpdate) -> Result<(), Error> {
        self.tree.show(update).map_err(Error::ShowScene)
    }

    /// The next deadline of the view after the last frame: when a timer fires or a motion comes to
    /// rest.
    ///
    /// # Errors
    /// Returns [`Error::NothingMoves`] when the view has no later deadline.
    pub fn next_deadline(&self) -> Result<Duration, Error> {
        (self.view.next_deadline())
            .filter(|deadline| *deadline > self.time)
            .ok_or(Error::NothingMoves)
    }

    /// Frames the view at its next deadline.
    ///
    /// # Errors
    /// Returns an error when the view has no later deadline or the frame cannot be shown.
    pub fn advance(&mut self) -> Result<(), Error> {
        self.time = self.next_deadline()?;
        self.frame()
    }

    /// Frames the view now, again while it needs a frame, and at each deadline until no motion
    /// runs, as a host that wakes at `View::next_deadline` does. A timer that fires after the last
    /// motion comes to rest remains pending.
    ///
    /// # Errors
    /// Returns an error when a motion runs without a deadline, when a frame at an unchanged time
    /// changes nothing in the scene and the view still needs another frame, or when a frame cannot be shown.
    pub fn settle(&mut self) -> Result<(), Error> {
        loop {
            let update = self.frame_update()?;
            let unchanged = update.changes.is_empty();
            self.show(update)?;
            if self.view.motion().is_active() {
                self.time = self.next_deadline()?;
            } else if !self.view.needs_frame() {
                return Ok(());
            } else if unchanged {
                return Err(Error::NoRest);
            }
        }
    }

    /// Frames the view every `interval` from now until it needs no frame and no motion runs, and
    /// passes each frame to `record`.
    ///
    /// # Errors
    /// Returns an error when a motion runs without a deadline, or when framing or `record` fails.
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

    /// Paints the scene of the last frame into the PNG file at `path`.
    ///
    /// # Errors
    /// Returns an error when the scene cannot be rendered or written.
    pub fn save(&mut self, path: impl AsRef<Path>) -> Result<(), Error> {
        (self.capture).paint(self.tree.at(self.time), self.viewport, path)
    }
}
