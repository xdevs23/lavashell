// SPDX-License-Identifier: GPL-3.0-only
use super::pipes::Scope;
use crate::Error;
use crate::app::Platform;
use crate::text_transfer::{TEXT_FORMATS, text_data};
use lavashell_core::surface::SurfaceId;
use sctk::reexports::client::protocol::wl_seat::WlSeat;
use smithay_client_toolkit as sctk;

impl Platform {
    pub(crate) fn copy_text(&mut self, text: &str) {
        if let Some(data) = text_data(text) {
            self.clipboard.queue.set_data(data, |result| {
                if let Err(error) = result {
                    log::warn!("the clipboard selection was not set: {error}");
                }
            });
        }
    }

    pub(crate) fn cut_text(&mut self, surface: SurfaceId) {
        let Some(source) = self
            .clipboard_view(surface)
            .and_then(|view| view.focus().and_then(|id| view.text_selection_source(id)))
        else {
            return;
        };
        if !source.can_move() {
            return;
        }
        let Some(data) = text_data(source.text()) else {
            return;
        };
        let actions = self.config.view_actions.clone();
        self.clipboard
            .queue
            .set_data(data, move |result| match result {
                Ok(()) => actions.dispatch(surface, move |view| {
                    if !view.complete_text_move(&source) {
                        log::debug!("the cut text stays in its field, which changed meanwhile");
                    }
                }),
                Err(error) => log::warn!("the cut text stays in its field: {error}"),
            });
    }

    pub(crate) fn paste_selection(&mut self, surface: SurfaceId, seat: &WlSeat) {
        let Some(target) = self.clipboard_view(surface).and_then(|view| {
            view.editing_target()
                .and_then(|id| view.text_paste_target(id))
        }) else {
            return;
        };
        let Some(target_surface) = self.clipboard_surface_for(surface) else {
            return;
        };
        let Some(offer) = self
            .clipboard
            .devices
            .iter()
            .find(|device| &device.seat == seat)
            .and_then(|device| device.device.data().selection_offer())
        else {
            return;
        };
        let Some(mime_type) = offer.with_mime_types(|types| {
            TEXT_FORMATS
                .iter()
                .find(|format| types.iter().any(|available| available == **format))
                .map(|format| (*format).into())
        }) else {
            return;
        };
        let pipe = match offer.receive(mime_type) {
            Ok(pipe) => pipe,
            Err(error) => {
                log::warn!("the clipboard text was not received: {error}");
                return;
            }
        };
        self.receive_transfer(
            pipe,
            Scope {
                seat: seat.clone(),
                surface: Some(target_surface.clone()),
                source: None,
                incoming: None,
            },
            Box::new(move |platform, bytes| {
                let text =
                    bytes.and_then(|bytes| String::from_utf8(bytes).map_err(Error::DecodeText));
                match text {
                    Ok(text) => {
                        if let Some(view) = platform.clipboard_view_for(&target_surface) {
                            let text: String = text
                                .chars()
                                .filter(|c| !c.is_control() || matches!(c, '\n' | '\r' | '\t'))
                                .collect();
                            if !view.paste_text_at(&target, &text) {
                                log::debug!("the pasted text was dropped, since its field changed");
                            }
                        }
                    }
                    Err(error) => log::warn!("the clipboard text was not read: {error}"),
                }
            }),
        );
    }
}
