// SPDX-License-Identifier: GPL-3.0-only
use crate::child_lines::{ChildLines, NextLine};
use crate::error::HarnessError;
use lavashell_process_scope::ScopedChild;
use lavashell_wayland_input::protocol::{Reply, Request};
use std::io::{ErrorKind, Write};

// https://gitlab.freedesktop.org/xkeyboard-config/xkeyboard-config/-/blob/master/keycodes/evdev
const SPACE_BAR: &str = "SPCE";

pub fn await_ready(replies: &mut ChildLines, helper: &ScopedChild) -> Result<u32, HarnessError> {
    match replies
        .next_line(Some(helper))
        .map_err(HarnessError::ReadHelperReply)?
    {
        NextLine::Line(line) => match read_reply(line)? {
            Reply::Ready { pid } => Ok(pid),
            reply => Err(HarnessError::HelperNotReady(reply)),
        },
        NextLine::End | NextLine::WatchedProcessExited => Err(HarnessError::HelperEndedBeforeReady),
    }
}

pub fn press_space_at(
    (x, y): (u32, u32),
    helper: &mut ScopedChild,
    replies: &mut ChildLines,
) -> Result<(), HarnessError> {
    let requests = [
        Request::Move { x, y },
        Request::Key {
            name: SPACE_BAR.to_owned(),
        },
    ];
    let mut input = helper.take_stdin().ok_or(HarnessError::NoHelperPipes)?;
    for request in &requests {
        let line = serde_json::to_string(request).map_err(HarnessError::EncodeHelperRequest)?;
        writeln!(input, "{line}").map_err(|error| match error.kind() {
            ErrorKind::BrokenPipe => HarnessError::HelperEndedBeforeCommands,
            _ => HarnessError::SendHelperRequest(error),
        })?;
    }
    drop(input);
    let mut received = Vec::new();
    while let NextLine::Line(line) = replies
        .next_line(None)
        .map_err(HarnessError::ReadHelperReply)?
    {
        received.push(read_reply(line)?);
    }
    if received.len() == requests.len() && received.iter().all(|reply| *reply == Reply::Done) {
        Ok(())
    } else {
        Err(HarnessError::HelperFailed(received))
    }
}

fn read_reply(line: String) -> Result<Reply, HarnessError> {
    line.parse()
        .map_err(|source| HarnessError::UnreadableHelperReply { line, source })
}
