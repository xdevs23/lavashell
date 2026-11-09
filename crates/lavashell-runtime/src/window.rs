// SPDX-License-Identifier: GPL-3.0-only
use crate::shape::WindowShape;
use crate::surface_records::{self, measure_content};
use lavashell_core::geometry::{Point, Size};
use lavashell_core::output::{HdrMetadata, OutputRequest};
use lavashell_core::units::Dp;
use lavashell_core::view::{Proposal, TextMeasurer, View};
use lavashell_platform::{Appearance, Role, SurfaceSpec, WindowRole};
use std::rc::Rc;

/// Whether a point in dp belongs to the drag region of a window with the given size.
pub type DragRegion = Rc<dyn Fn(Point, Size) -> bool>;

#[derive(Clone)]
pub struct Window {
    pub title: String,
    pub output: OutputRequest,
    pub hdr_metadata: Option<HdrMetadata>,
    pub background_blur: bool,
    pub transparent: bool,
    pub shape: Option<WindowShape>,
    pub drag_region: Option<DragRegion>,
    /// The size of the window when it opens, or `None` for the size measured by the view without a size proposal.
    pub size: Option<Size>,
    /// The id of the app, by which the host groups its windows and finds its name and icon.
    pub app_id: String,
    pub min_size: Option<Size>,
    pub fullscreen: bool,
}

impl Window {
    pub fn new(title: impl Into<String>, app_id: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            output: OutputRequest::default(),
            hdr_metadata: None,
            background_blur: false,
            transparent: false,
            shape: None,
            drag_region: None,
            size: None,
            app_id: app_id.into(),
            min_size: None,
            fullscreen: false,
        }
    }

    #[must_use]
    pub fn output(mut self, output: OutputRequest) -> Self {
        self.output = output;
        self
    }

    /// Asks the host to blur what is behind the whole surface. A host without background blur shows the transparent
    /// surface without blur. Backdrop filters inside the scene work independently of this setting.
    #[must_use]
    pub fn background_blur(mut self, enabled: bool) -> Self {
        self.background_blur = enabled;
        self
    }

    #[must_use]
    pub fn hdr_metadata(mut self, metadata: HdrMetadata) -> Self {
        self.hdr_metadata = Some(metadata);
        self
    }

    #[must_use]
    pub fn transparent(mut self, transparent: bool) -> Self {
        self.transparent = transparent;
        self
    }

    /// Clips the window to the outline of `shape`. Pointer and touch events outside the outline pass through to
    /// the surfaces below. A shaped window gets a transparent surface and draws its decorations itself.
    #[must_use]
    pub fn shape(mut self, shape: WindowShape) -> Self {
        self.shape = Some(shape);
        self
    }

    /// Sets the region in which a press of the primary button starts a window move by the host. `region` returns
    /// whether a point in dp belongs to the region of a window with the given size. The region usually covers the
    /// header of the window without the interactive controls in it.
    #[must_use]
    pub fn drag_region(mut self, region: impl Fn(Point, Size) -> bool + 'static) -> Self {
        self.drag_region = Some(Rc::new(region));
        self
    }

    #[must_use]
    pub fn size(mut self, width: Dp, height: Dp) -> Self {
        self.size = Some(Size::new(width, height));
        self
    }

    #[must_use]
    pub fn min_size(mut self, width: Dp, height: Dp) -> Self {
        self.min_size = Some(Size::new(width, height));
        self
    }

    /// Opens the window fullscreen. Some hosts enable adaptive sync only for fullscreen windows.
    #[must_use]
    pub fn fullscreen(mut self, enabled: bool) -> Self {
        self.fullscreen = enabled;
        self
    }

    /// The native surface of the window that shows `view`. The window opens at its requested size, or else at the
    /// size that `view` fits without bounds.
    ///
    /// # Errors
    ///
    /// Fails when the view cannot be measured.
    pub fn spec(
        &self,
        view: &mut View,
        text: &mut impl TextMeasurer,
    ) -> Result<SurfaceSpec, surface_records::Error> {
        let unbounded = Proposal {
            width: None,
            height: None,
        };
        let size = match self.size {
            Some(size) => size,
            None => measure_content(view, unbounded, text)?,
        };
        Ok(SurfaceSpec {
            size,
            appearance: Appearance {
                transparent: self.transparent || self.shape.is_some(),
                output: self.output,
                hdr_metadata: self.hdr_metadata,
                background_blur: self.background_blur,
            },
            role: Role::Window(WindowRole {
                title: self.title.clone(),
                app_id: self.app_id.clone(),
                min_size: self.min_size,
                fullscreen: self.fullscreen,
                draws_decorations: self.shape.is_some(),
            }),
        })
    }
}

#[cfg(test)]
mod tests;
