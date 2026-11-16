// SPDX-License-Identifier: GPL-3.0-only
use lavashell_process_scope::{ProcessError, ScopedChild};
use rustix::process::{Pid, RawPid};
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

const CHILD_IGNORING_TERM: &str = "trap '' TERM; exec sleep infinity";

#[derive(Debug, thiserror::Error)]
pub enum FixtureError {
    #[error(transparent)]
    Process(#[from] ProcessError),
    #[error("the wrapper was started without a pipe for its output")]
    NoOutputPipe,
    #[error("the output of the wrapper cannot be read")]
    ReadOutput(#[source] std::io::Error),
    #[error("the wrapper wrote {0:?} in place of process ids")]
    UnreadablePids(String),
}

#[derive(Clone, Copy)]
pub enum WrapperEnd {
    WaitsForChild,
    ExitsAtOnce,
}

impl WrapperEnd {
    pub fn script(self) -> String {
        let end = match self {
            Self::WaitsForChild => "wait",
            Self::ExitsAtOnce => "exit 0",
        };
        format!("sh -c \"{CHILD_IGNORING_TERM}\" & echo $!; {end}")
    }
}

pub fn spawn_wrapper(end: WrapperEnd) -> Result<(ScopedChild, Pid), FixtureError> {
    let mut wrapper = ScopedChild::spawn(
        Command::new("sh")
            .args(["-c", &end.script()])
            .stdout(Stdio::piped()),
    )?;
    let [child] = read_pids(&mut wrapper)?;
    Ok((wrapper, child))
}

pub fn read_pids<const COUNT: usize>(
    process: &mut ScopedChild,
) -> Result<[Pid; COUNT], FixtureError> {
    let output = process.take_stdout().ok_or(FixtureError::NoOutputPipe)?;
    let mut line = String::new();
    let _line_length =
        (BufReader::new(output).read_line(&mut line)).map_err(FixtureError::ReadOutput)?;
    parse_pids(line)
}

pub fn parse_pids<const COUNT: usize>(line: String) -> Result<[Pid; COUNT], FixtureError> {
    let pids: Option<Vec<Pid>> = (line.split_whitespace())
        .map(|word| word.parse::<RawPid>().ok().and_then(Pid::from_raw))
        .collect();
    pids.and_then(|pids| pids.try_into().ok())
        .ok_or(FixtureError::UnreadablePids(line))
}
