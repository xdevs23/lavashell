// SPDX-License-Identifier: GPL-3.0-only
//! The module paths of the ndk crate used by the stream. Each path re-exports the simulated type in its place.

pub mod hardware_buffer {
    pub use crate::platform::image::{HardwareBuffer, HardwareBufferRef, HardwareBufferUsage};
}

pub mod hardware_buffer_format {
    pub use crate::platform::image::HardwareBufferFormat;
}

pub mod media {
    pub mod image_reader {
        pub use crate::platform::image::{AcquireResult, CropRect, Image, ImageFormat};
        pub use crate::platform::reader::ImageReader;
    }
}

pub mod media_error {
    pub use crate::platform::image::MediaError;
}

pub mod native_window {
    pub use crate::platform::NativeWindow;
}
