// SPDX-License-Identifier: GPL-3.0-only
use crate::shape::WindowShape;
use crate::surface_records::{self, measure_content};
use lavashell_core::geometry::Size;
use lavashell_core::output::{HdrMetadata, OutputRequest};
use lavashell_core::units::Dp;
use lavashell_core::view::{Proposal, TextMeasurer, View};
use lavashell_platform::{Role, SurfaceAppearance, SurfaceSpec, WindowRole};

#[derive(Clone)]
pub struct Window {
    pub title: String,
    pub output: OutputRequest,
    pub hdr_metadata: Option<HdrMetadata>,
    pub background_blur: bool,
    pub transparent: bool,
    pub shape: Option<WindowShape>,
    pub size: Option<Size>,
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

    #[must_use]
    pub fn shape(mut self, shape: WindowShape) -> Self {
        self.shape = Some(shape);
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

    #[must_use]
    pub fn fullscreen(mut self, enabled: bool) -> Self {
        self.fullscreen = enabled;
        self
    }

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
            appearance: SurfaceAppearance {
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
