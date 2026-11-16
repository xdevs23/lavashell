// SPDX-License-Identifier: GPL-3.0-only
use crate::error::ProcessError;
use crate::scoped_child::ScopedChild;
use std::fmt;
use std::io::{self, Read};
use std::process::{ChildStdout, Command, Stdio};
use std::thread::ScopedJoinHandle;

#[derive(Debug)]
pub struct CommandLine(Vec<String>);

impl From<&Command> for CommandLine {
    fn from(command: &Command) -> Self {
        Self(
            std::iter::once(command.get_program())
                .chain(command.get_args())
                .map(|word| word.to_string_lossy().into_owned())
                .collect(),
        )
    }
}

impl fmt::Display for CommandLine {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0.join(" "))
    }
}

trait StandardOutput {
    type Content: Send;

    fn destination() -> Stdio;

    fn read(pipe: Option<ChildStdout>) -> io::Result<Self::Content>;
}

enum DiscardedOutput {}

impl StandardOutput for DiscardedOutput {
    type Content = ();

    fn destination() -> Stdio {
        Stdio::null()
    }

    fn read(_pipe_of_null_output: Option<ChildStdout>) -> io::Result<()> {
        Ok(())
    }
}

enum CapturedOutput {}

impl StandardOutput for CapturedOutput {
    type Content = Vec<u8>;

    fn destination() -> Stdio {
        Stdio::piped()
    }

    fn read(pipe: Option<ChildStdout>) -> io::Result<Vec<u8>> {
        read_to_end(pipe)
    }
}

pub fn run_checked(command: &mut Command) -> Result<(), ProcessError> {
    run_with::<DiscardedOutput>(command)
}

pub fn run_checked_for_stdout(command: &mut Command) -> Result<Vec<u8>, ProcessError> {
    run_with::<CapturedOutput>(command)
}

fn run_with<Output: StandardOutput>(
    command: &mut Command,
) -> Result<Output::Content, ProcessError> {
    let mut child = ScopedChild::spawn(
        command
            .stdin(Stdio::null())
            .stdout(Output::destination())
            .stderr(Stdio::piped()),
    )?;
    let (stdout, stderr) = (child.take_stdout(), child.take_stderr());
    let (exit, stdout, stderr) = std::thread::scope(|scope| {
        let stdout = scope.spawn(|| Output::read(stdout));
        let stderr = scope.spawn(|| read_to_end(stderr));
        let exit = child.wait();
        drop(child);
        (exit, join(stdout), join(stderr))
    });
    let collect_failed = |source| ProcessError::CollectOutput {
        command: CommandLine::from(&*command),
        source,
    };
    let (exit, stdout, stderr) = (
        exit?,
        stdout.map_err(collect_failed)?,
        stderr.map_err(collect_failed)?,
    );
    if exit.success() {
        Ok(stdout)
    } else {
        Err(ProcessError::Failed {
            command: CommandLine::from(&*command),
            exit,
            stderr: String::from_utf8_lossy(&stderr).into_owned(),
        })
    }
}

fn read_to_end(pipe: Option<impl Read>) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    if let Some(mut pipe) = pipe {
        let _byte_count = pipe.read_to_end(&mut bytes)?;
    }
    Ok(bytes)
}

fn join<T>(reader: ScopedJoinHandle<'_, T>) -> T {
    reader
        .join()
        .unwrap_or_else(|payload| std::panic::resume_unwind(payload))
}
