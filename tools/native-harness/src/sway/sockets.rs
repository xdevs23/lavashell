// SPDX-License-Identifier: GPL-3.0-only
use crate::error::HarnessError;
use lavashell_process_scope::{ScopedChild, first_ready};
use rustix::fs::inotify;
use std::ffi::{CStr, OsString};
use std::mem::MaybeUninit;
use std::os::fd::{AsFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

// https://man7.org/linux/man-pages/man7/inotify.7.html
const EVENT_HEADER_LENGTH: usize = 4 * size_of::<u32>();
// https://man7.org/linux/man-pages/man3/limits.h.0p.html
const NAME_MAX: usize = 255;
// https://github.com/swaywm/sway/blob/1.11/sway/ipc-server.c#L141
const IPC_SOCKET_PREFIX: &[u8] = b"sway-ipc.";
const IPC_SOCKET_SUFFIX: &[u8] = b".sock";
// https://gitlab.freedesktop.org/wayland/wayland/-/blob/1.24.0/src/wayland-server.c#L1836
const DISPLAY_SOCKET_PREFIX: &[u8] = b"wayland-";
// https://gitlab.freedesktop.org/wayland/wayland/-/blob/1.24.0/src/wayland-server.c#L60
const DISPLAY_LOCK_SUFFIX: &[u8] = b".lock";

pub struct SwaySockets {
    runtime: PathBuf,
    events: OwnedFd,
}

pub struct FoundSockets {
    pub ipc: PathBuf,
    pub display: OsString,
}

#[derive(Default)]
struct SocketNames {
    ipc: Option<OsString>,
    display: Option<OsString>,
}

#[derive(Clone, Copy)]
enum SocketWake {
    Created,
    CompositorExited,
}

impl SwaySockets {
    pub fn watch(runtime: &Path) -> Result<Self, HarnessError> {
        let events =
            inotify::init(inotify::CreateFlags::CLOEXEC).map_err(HarnessError::WatchRuntime)?;
        let _watch_descriptor = inotify::add_watch(&events, runtime, inotify::WatchFlags::CREATE)
            .map_err(HarnessError::WatchRuntime)?;
        Ok(Self {
            runtime: runtime.to_owned(),
            events,
        })
    }

    pub fn wait(self, compositor: &ScopedChild) -> Result<FoundSockets, HarnessError> {
        let mut buffer = [MaybeUninit::uninit(); EVENT_HEADER_LENGTH + NAME_MAX + 1];
        let mut reader = inotify::Reader::new(self.events.as_fd(), &mut buffer);
        let mut names = SocketNames::default();
        loop {
            if let (Some(ipc), Some(display)) = (&names.ipc, &names.display) {
                return Ok(FoundSockets {
                    ipc: self.runtime.join(ipc),
                    display: display.clone(),
                });
            }
            if reader.is_buffer_empty() {
                let sources = [
                    (self.events.as_fd(), SocketWake::Created),
                    (compositor.exit_event(), SocketWake::CompositorExited),
                ];
                match first_ready(&sources)? {
                    SocketWake::CompositorExited => return Err(HarnessError::CompositorExited),
                    SocketWake::Created => {}
                }
            }
            let event = reader.next().map_err(HarnessError::WatchRuntime)?;
            if let Some(name) = event.file_name() {
                names.note(name)?;
            }
        }
    }
}

impl SocketNames {
    fn note(&mut self, name: &CStr) -> Result<(), HarnessError> {
        let bytes = name.to_bytes();
        let slot = if bytes.starts_with(IPC_SOCKET_PREFIX) && bytes.ends_with(IPC_SOCKET_SUFFIX) {
            &mut self.ipc
        } else if bytes.starts_with(DISPLAY_SOCKET_PREFIX) && !bytes.ends_with(DISPLAY_LOCK_SUFFIX)
        {
            &mut self.display
        } else {
            return Ok(());
        };
        let name = std::ffi::OsStr::from_bytes(bytes).to_owned();
        match slot.replace(name.clone()) {
            None => Ok(()),
            Some(earlier) => Err(HarnessError::AmbiguousSockets {
                earlier,
                later: name,
            }),
        }
    }
}
