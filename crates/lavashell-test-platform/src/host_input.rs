// SPDX-License-Identifier: GPL-3.0-only
use crate::Error;
use crate::linux_key_code::convert_to_linux_key_code;
use lavashell_actor::{Receiver, Sender, mailbox};
use lavashell_core::keyboard::{Key, KeyStroke};
use lavashell_core::surface::SurfaceId;
use lavashell_platform::{DeviceEvent, KeyAction, KeyboardDevice, PlatformEvent};
use std::sync::Arc;

const KEYBOARD: KeyboardDevice = KeyboardDevice(1);

#[derive(Clone)]
pub struct HostInput {
    events: Sender<PlatformEvent>,
    reported: Arc<Receiver<PlatformEvent>>,
}

impl Default for HostInput {
    fn default() -> Self {
        let (events, reported) = mailbox();
        Self {
            events,
            reported: Arc::new(reported),
        }
    }
}

impl HostInput {
    pub fn report(&self, event: impl Into<PlatformEvent>) {
        let _open_while_the_input_has_a_sender = self.events.push(event.into());
    }

    pub fn press(&self, surface: SurfaceId, keys: &[Key]) -> Result<(), Error> {
        let coded: Vec<_> = (keys.iter())
            .map(|&key| {
                Ok((
                    convert_to_linux_key_code(key).ok_or(Error::Press { key })?,
                    key,
                ))
            })
            .collect::<Result<_, Error>>()?;
        self.report(DeviceEvent::KeyboardFocus {
            keyboard: KEYBOARD,
            surface: Some(surface),
        });
        for (code, key) in coded {
            for action in [KeyAction::Pressed, KeyAction::Released] {
                self.report(DeviceEvent::Key {
                    keyboard: KEYBOARD,
                    code,
                    stroke: KeyStroke {
                        key: Some(key),
                        text: None,
                    },
                    action,
                });
            }
        }
        Ok(())
    }

    pub(crate) fn take(&self) -> Vec<PlatformEvent> {
        (self.reported.take_arrived().into_iter().flatten()).collect()
    }
}
