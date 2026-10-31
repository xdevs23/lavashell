// SPDX-License-Identifier: GPL-3.0-only
use super::{
    ReadCompletion, Request,
    pipes::Scope,
    sources::{Payload, Source},
};
use crate::app::Platform;
use crate::{Cancellation, Error};
use lavashell_core::surface::SurfaceId;
use lavashell_core::transfer::{ReceivedData, TransferData};
use sctk::reexports::client::protocol::wl_seat::WlSeat;
use smithay_client_toolkit as sctk;

impl Platform {
    pub(crate) fn submit_transfers(&mut self) {
        self.cancel_closed_transfers();
        let requests: Vec<_> = self
            .clipboard
            .queue
            .0
            .borrow_mut()
            .requests
            .drain(..)
            .collect();
        for request in requests {
            match request {
                Request::Set(data, done) => done(self.offer_selection(data)),
                Request::Read {
                    surface,
                    formats,
                    done,
                } => {
                    let seat = if self.has_surface(surface) {
                        self.clipboard_input_device(surface)
                    } else {
                        Err(Cancellation::SurfaceClosed.into())
                    };
                    match seat {
                        Ok(seat) => self.read_selection(surface, &seat, &formats, done),
                        Err(error) => done(Err(error)),
                    }
                }
            }
        }
    }

    fn offer_selection(&mut self, data: TransferData) -> Result<(), Error> {
        let manager = (self.clipboard.manager.as_ref()).ok_or(Error::ClipboardUnavailable)?;
        let (seat, serial) = (self.clipboard.serial.as_ref()).ok_or(Error::NoInputSerial)?;
        let device = (self.clipboard.devices.iter())
            .find(|device| &device.seat == seat)
            .ok_or(Cancellation::InputDeviceGone)?;
        let source = manager.create_copy_paste_source(&self.qh, data.formats());
        source.set_selection(&device.device, *serial);
        self.clipboard.payloads.push(Payload {
            source: Source::Selection(source),
            data,
            seat: seat.clone(),
            drag: None,
        });
        Ok(())
    }

    pub(crate) fn read_selection(
        &mut self,
        surface: SurfaceId,
        seat: &WlSeat,
        formats: &[String],
        done: ReadCompletion,
    ) {
        let result = (|| {
            let target = self
                .clipboard_surface_for(surface)
                .ok_or(Cancellation::SurfaceClosed)?;
            let offer = self
                .clipboard
                .devices
                .iter()
                .find(|device| &device.seat == seat)
                .and_then(|device| device.device.data().selection_offer())
                .ok_or(Error::ClipboardEmpty)?;
            let mime_type = offer
                .with_mime_types(|available| {
                    formats
                        .iter()
                        .find(|format| available.contains(format))
                        .cloned()
                })
                .ok_or(Error::NoRequestedFormat)?;
            let pipe = offer
                .receive(mime_type.clone())
                .map_err(Error::ReceiveSelection)?;
            Ok((target, mime_type, pipe))
        })();
        match result {
            Ok((target, mime_type, pipe)) => self.receive_transfer(
                pipe,
                Scope {
                    seat: seat.clone(),
                    surface: Some(target),
                    source: None,
                    incoming: None,
                },
                Box::new(move |_, bytes| {
                    done(bytes.map(|bytes| ReceivedData { mime_type, bytes }));
                }),
            ),
            Err(error) => done(Err(error)),
        }
    }
}
