// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::surface::SurfaceId;
use lavashell_core::window::WindowRequest;
use lavashell_platform::{PointerPress, WindowControlRequest};

pub(crate) fn to_window_control_request(
    surface: SurfaceId,
    request: WindowRequest,
    last_press: Option<PointerPress>,
) -> Option<WindowControlRequest> {
    Some(match request {
        WindowRequest::SetMaximized(maximized) => {
            WindowControlRequest::SetMaximized { surface, maximized }
        }
        WindowRequest::Minimize => WindowControlRequest::Minimize { surface },
        WindowRequest::SetFullscreen(fullscreen) => WindowControlRequest::SetFullscreen {
            surface,
            fullscreen,
        },
        WindowRequest::ShowMenu(at) => {
            let Some(press) = last_press else {
                log::warn!(
                    "the window menu of {surface:?} does not open: the last pointer press is unknown"
                );
                return None;
            };
            WindowControlRequest::ShowMenu { surface, press, at }
        }
    })
}

#[cfg(test)]
mod tests;
