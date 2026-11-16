// SPDX-License-Identifier: GPL-3.0-only
use crate::error::SessionError;
use crate::frame::{FrameLayout, PendingFrame};
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::reexports::client::{
    Connection, Dispatch, QueueHandle, WEnum, delegate_noop,
    protocol::{wl_buffer::WlBuffer, wl_output},
};
use smithay_client_toolkit::reexports::protocols_wlr::screencopy::v1::client::{
    zwlr_screencopy_frame_v1::{self, ZwlrScreencopyFrameV1},
    zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1,
};
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::shm::{Shm, ShmHandler, raw::RawPool};
use smithay_client_toolkit::{delegate_registry, registry_handlers};
use std::path::Path;

pub struct ProtocolState {
    pub registry: RegistryState,
    pub outputs: OutputState,
    pub shm: Shm,
    pub pool: Option<RawPool>,
    pub frame: PendingFrame,
}

impl ProtocolState {
    pub fn only_output(&self) -> Result<wl_output::WlOutput, SessionError> {
        let mut outputs = self.outputs.outputs();
        match (outputs.next(), outputs.next()) {
            (Some(output), None) => Ok(output),
            (None, _) => Err(SessionError::NoOutput),
            (Some(_), Some(_)) => Err(SessionError::AmbiguousOutput),
        }
    }

    pub fn write_pixels(&mut self, layout: FrameLayout, path: &Path) -> Result<(), SessionError> {
        let length = layout.byte_length()?;
        let pool = (self.pool.as_mut()).ok_or(SessionError::ReadyBeforeCopy)?;
        std::fs::write(path, &pool.mmap()[..length]).map_err(SessionError::WritePixels)
    }

    fn copy(
        &mut self,
        frame: &ZwlrScreencopyFrameV1,
        qh: &QueueHandle<Self>,
    ) -> Result<(), SessionError> {
        let layout = self.frame.take_offer()?;
        let length = layout.byte_length()?;
        let pool = match &mut self.pool {
            Some(pool) if pool.len() >= length => pool,
            Some(pool) => {
                pool.resize(length).map_err(SessionError::ResizePool)?;
                pool
            }
            None => self
                .pool
                .insert(RawPool::new(length, &self.shm).map_err(SessionError::CreatePool)?),
        };
        let [width, height, stride] = layout.wire_lengths()?;
        let buffer = pool.create_buffer(0, width, height, stride, layout.format.to_wire(), (), qh);
        frame.copy_with_damage(&buffer);
        self.frame.copy_into(layout, buffer);
        Ok(())
    }
}

impl ProvidesRegistryState for ProtocolState {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }

    registry_handlers![OutputState];
}

impl ShmHandler for ProtocolState {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl OutputHandler for ProtocolState {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }

    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}

    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}

    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

delegate_registry!(ProtocolState);

smithay_client_toolkit::delegate_dispatch2!(ProtocolState);

delegate_noop!(ProtocolState: ignore ZwlrScreencopyManagerV1);

delegate_noop!(ProtocolState: ignore WlBuffer);

impl Dispatch<ZwlrScreencopyFrameV1, ()> for ProtocolState {
    fn event(
        state: &mut Self,
        frame: &ZwlrScreencopyFrameV1,
        event: zwlr_screencopy_frame_v1::Event,
        (): &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        use zwlr_screencopy_frame_v1::{Event, Flags};
        match event {
            Event::Buffer {
                format,
                width,
                height,
                stride,
            } => state
                .frame
                .offer(FrameLayout::offered(format, width, height, stride)),
            Event::Flags { flags } => state
                .frame
                .invert(matches!(flags, WEnum::Value(flags) if flags.contains(Flags::YInvert))),
            Event::BufferDone => {
                if let Err(failure) = state.copy(frame, qh) {
                    state.frame.fail(failure);
                }
            }
            Event::Ready { .. } => state.frame.finish(),
            Event::Failed => state.frame.fail(SessionError::CopyFailed),
            Event::Damage { .. } | Event::LinuxDmabuf { .. } | _ => {}
        }
    }
}
