// SPDX-License-Identifier: GPL-3.0-only
use crate::Error;
use crate::platform::ndk;
use lavashell_core::prelude::PixelSize;
use lavashell_platform::SurfaceControl;
use std::convert::Infallible;

#[derive(Clone)]
pub struct NativeWindow {
    pub(crate) window: ndk::native_window::NativeWindow,
}

impl NativeWindow {
    #[must_use]
    pub fn new(window: ndk::native_window::NativeWindow) -> Self {
        Self { window }
    }

    pub fn size(&self) -> Result<PixelSize, Error> {
        let (width, height) = (self.window.width(), self.window.height());
        match (u32::try_from(width), u32::try_from(height)) {
            (Ok(width), Ok(height)) => Ok(PixelSize::new(width, height)),
            _ => Err(Error::WindowSize { width, height }),
        }
    }
}

impl SurfaceControl for NativeWindow {
    type Error = Infallible;
    type Message = Infallible;

    fn receive(&mut self, message: Infallible) {
        match message {}
    }
}
