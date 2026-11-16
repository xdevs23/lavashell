// SPDX-License-Identifier: GPL-3.0-only
use crate::error::SessionError;
use serde::Serialize;
use smithay_client_toolkit::reexports::client::{
    WEnum,
    protocol::{wl_buffer::WlBuffer, wl_shm},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PixelFormat {
    Xrgb8888,
    Argb8888,
}

impl PixelFormat {
    fn read(format: WEnum<wl_shm::Format>) -> Result<Self, SessionError> {
        match format {
            WEnum::Value(wl_shm::Format::Xrgb8888) => Ok(Self::Xrgb8888),
            WEnum::Value(wl_shm::Format::Argb8888) => Ok(Self::Argb8888),
            unsupported => Err(SessionError::UnsupportedFormat(unsupported)),
        }
    }

    pub fn to_wire(self) -> wl_shm::Format {
        match self {
            Self::Xrgb8888 => wl_shm::Format::Xrgb8888,
            Self::Argb8888 => wl_shm::Format::Argb8888,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameLayout {
    pub format: PixelFormat,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
}

impl FrameLayout {
    pub fn offered(
        format: WEnum<wl_shm::Format>,
        width: u32,
        height: u32,
        stride: u32,
    ) -> Result<Self, SessionError> {
        Ok(Self {
            format: PixelFormat::read(format)?,
            width,
            height,
            stride,
        })
    }

    pub fn byte_length(self) -> Result<usize, SessionError> {
        let length = |value: u32| usize::try_from(value).map_err(SessionError::FrameOutOfRange);
        (length(self.stride)?.checked_mul(length(self.height)?)).ok_or(SessionError::FrameTooLarge)
    }

    pub fn wire_lengths(self) -> Result<[i32; 3], SessionError> {
        let length = |value: u32| i32::try_from(value).map_err(SessionError::FrameOutOfRange);
        Ok([
            length(self.width)?,
            length(self.height)?,
            length(self.stride)?,
        ])
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapturedFrame {
    pub layout: FrameLayout,
    pub inverted: bool,
}

#[derive(Default)]
pub struct PendingFrame {
    offered: Option<Result<FrameLayout, SessionError>>,
    inverted: bool,
    copying: Option<(FrameLayout, WlBuffer)>,
    outcome: Option<Result<CapturedFrame, SessionError>>,
}

impl PendingFrame {
    pub fn offer(&mut self, layout: Result<FrameLayout, SessionError>) {
        self.offered = Some(layout);
    }

    pub fn invert(&mut self, inverted: bool) {
        self.inverted = inverted;
    }

    pub fn take_offer(&mut self) -> Result<FrameLayout, SessionError> {
        (self.offered.take()).unwrap_or(Err(SessionError::NoSharedMemoryBuffer))
    }

    pub fn copy_into(&mut self, layout: FrameLayout, buffer: WlBuffer) {
        self.copying = Some((layout, buffer));
    }

    pub fn finish(&mut self) {
        let captured = match &self.copying {
            Some((layout, _)) => Ok(CapturedFrame {
                layout: *layout,
                inverted: self.inverted,
            }),
            None => Err(SessionError::ReadyBeforeCopy),
        };
        self.outcome = Some(captured);
    }

    pub fn fail(&mut self, failure: SessionError) {
        self.outcome = Some(Err(failure));
    }

    pub fn take_outcome(&mut self) -> Option<Result<CapturedFrame, SessionError>> {
        self.outcome.take()
    }

    pub fn release(self) {
        if let Some((_, buffer)) = self.copying {
            buffer.destroy();
        }
    }
}
