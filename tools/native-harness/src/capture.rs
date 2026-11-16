// SPDX-License-Identifier: GPL-3.0-only
use crate::child_lines::{ChildLines, NextLine};
use crate::error::HarnessError;
use crate::sway::HeadlessSway;
use lavashell_output_capture::protocol::{Reply, Request};
use lavashell_process_scope::ScopedChild;
use std::io::Write;
use std::path::Path;
use std::process::{ChildStdin, Command, Stdio};

pub const SUBCOMMAND: &str = "capture-output";

pub struct OutputCapture {
    replies: ChildLines,
    requests: ChildStdin,
    session: ScopedChild,
}

impl OutputCapture {
    pub fn start(sway: &HeadlessSway) -> Result<Self, HarnessError> {
        let executable = std::env::current_exe().map_err(HarnessError::LocateExecutable)?;
        let mut session = ScopedChild::spawn(
            Command::new(executable)
                .arg(SUBCOMMAND)
                .env_clear()
                .envs(sway.environment())
                .stdin(Stdio::piped())
                .stdout(Stdio::piped()),
        )?;
        let requests = session.take_stdin().ok_or(HarnessError::NoCapturePipes)?;
        let replies = session.take_stdout().ok_or(HarnessError::NoCapturePipes)?;
        Ok(Self {
            replies: ChildLines::of(replies).map_err(HarnessError::ReadCaptureReply)?,
            requests,
            session,
        })
    }

    pub fn capture_change(
        &mut self,
        image: &Path,
        client: Option<&ScopedChild>,
    ) -> Result<(), HarnessError> {
        let request = Request::CaptureChange {
            image: image.to_owned(),
        };
        let line = serde_json::to_string(&request).map_err(HarnessError::EncodeCaptureRequest)?;
        writeln!(self.requests, "{line}")
            .and_then(|()| self.requests.flush())
            .map_err(HarnessError::SendCaptureRequest)?;
        let reply = match self
            .replies
            .next_line(client)
            .map_err(HarnessError::ReadCaptureReply)?
        {
            NextLine::Line(line) => line,
            NextLine::End => return Err(HarnessError::CaptureEnded(self.session.wait()?)),
            NextLine::WatchedProcessExited => {
                return Err(HarnessError::AppEndedBeforeItsOutputChanged);
            }
        };
        match reply.parse::<Reply>() {
            Ok(Reply::Frame { .. }) => Ok(()),
            Ok(Reply::Failed { message }) => Err(HarnessError::CaptureFailed { message }),
            Err(source) => Err(HarnessError::UnreadableCaptureReply {
                line: reply,
                source,
            }),
        }
    }

    pub fn capture_change_from(
        &mut self,
        image: &Path,
        reference: &[u8],
        client: &ScopedChild,
    ) -> Result<(), HarnessError> {
        loop {
            self.capture_change(image, Some(client))?;
            let captured = std::fs::read(image).map_err(|source| HarnessError::ReadImage {
                path: image.to_owned(),
                source,
            })?;
            if captured != reference {
                return Ok(());
            }
        }
    }
}
