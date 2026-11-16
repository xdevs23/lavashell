// SPDX-License-Identifier: GPL-3.0-only
use lavashell_process_scope::{ProcessError, ScopedChild, first_ready};
use std::io::{BufRead, BufReader};
use std::os::fd::{AsFd, OwnedFd};
use std::process::ChildStdout;

pub struct ChildLines {
    reader: BufReader<ChildStdout>,
    readiness: OwnedFd,
}

pub enum NextLine {
    Line(String),
    End,
    WatchedProcessExited,
}

#[derive(Debug, thiserror::Error)]
pub enum ChildLinesError {
    #[error("the output of the process cannot be read")]
    Read(#[source] std::io::Error),
    #[error(transparent)]
    Wait(#[from] ProcessError),
}

#[derive(Clone, Copy)]
enum LineWake {
    Output,
    WatchedProcessExited,
}

impl ChildLines {
    pub fn of(output: ChildStdout) -> Result<Self, ChildLinesError> {
        let readiness = output
            .as_fd()
            .try_clone_to_owned()
            .map_err(ChildLinesError::Read)?;
        Ok(Self {
            reader: BufReader::new(output),
            readiness,
        })
    }

    pub fn next_line(
        &mut self,
        watched: Option<&ScopedChild>,
    ) -> Result<NextLine, ChildLinesError> {
        if self.reader.buffer().is_empty() {
            let sources: Vec<_> = std::iter::once((self.readiness.as_fd(), LineWake::Output))
                .chain(
                    watched.map(|process| (process.exit_event(), LineWake::WatchedProcessExited)),
                )
                .collect();
            match first_ready(&sources)? {
                LineWake::WatchedProcessExited => return Ok(NextLine::WatchedProcessExited),
                LineWake::Output => {}
            }
        }
        let mut line = String::new();
        match self
            .reader
            .read_line(&mut line)
            .map_err(ChildLinesError::Read)?
        {
            0 => Ok(NextLine::End),
            _line_length => Ok(NextLine::Line(line)),
        }
    }
}
