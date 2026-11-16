// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

mod fixture;

use fixture::{FixtureError, WrapperEnd, read_pids, spawn_wrapper};
use lavashell_process_scope::{
    ProcessError, ProcessExit, RunOutcome, ScopedChild, run_ending_groups_on_termination,
};
use rustix::io::Errno;
use rustix::process::{Signal, kill_process, test_kill_process};
use std::ffi::OsString;
use std::fs;
use std::io::{self, BufRead, BufReader, Lines, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

#[derive(Debug, thiserror::Error)]
enum TerminationTestError {
    #[error(transparent)]
    Fixture(#[from] FixtureError),
    #[error(transparent)]
    Process(#[from] ProcessError),
    #[error("this test cannot find the path of its own binary")]
    LocateExecutable(#[source] std::io::Error),
    #[error("the harness fixture started without the directory it needs for files")]
    MissingFixtureDirectory,
    #[error("this test cannot use the directory for the files of the harness fixture")]
    FixtureDirectory(#[source] std::io::Error),
    #[error("the harness fixture started a process after the termination signal")]
    StartAccepted,
    #[error("this test cannot send a signal to the harness fixture")]
    Signal(#[source] Errno),
    #[error("this test cannot read the output from the harness fixture")]
    ReadFixtureOutput(#[source] io::Error),
    #[error("the harness fixture printed {line:?} where this test expected {expected:?}")]
    UnexpectedFixtureLine {
        expected: &'static str,
        line: Option<String>,
    },
    #[error("the harness fixture cannot read its standard input")]
    ReadInput(#[source] io::Error),
}

const FIXTURE_ARGUMENT: &str = "harness-fixture";
const HANGING_FIXTURE_ARGUMENT: &str = "hanging-cleanup-fixture";
const READY_LINE: &str = "ready";
const CLEANING_UP_LINE: &str = "cleaning up";
const REFUSAL_RECORD: &str = "start-refused";
const TERMINATED: ProcessExit = ProcessExit::Code(128 + Signal::TERM.as_raw());
const INTERRUPTED: ProcessExit = ProcessExit::Code(128 + Signal::INT.as_raw());

fn main() -> ExitCode {
    let mut arguments = std::env::args_os().skip(1);
    let outcome = match arguments.next() {
        Some(argument) if argument == FIXTURE_ARGUMENT => arguments
            .next()
            .map(PathBuf::from)
            .ok_or(TerminationTestError::MissingFixtureDirectory)
            .and_then(|directory| run_harness_fixture(&directory)),
        Some(argument) if argument == HANGING_FIXTURE_ARGUMENT => run_hanging_fixture(),
        _ => run_tests().map(|()| ExitCode::SUCCESS),
    };
    outcome.unwrap_or_else(|error| {
        eprintln!("{error}");
        ExitCode::FAILURE
    })
}

fn run_tests() -> Result<(), TerminationTestError> {
    a_termination_signal_ends_the_groups_and_lets_the_work_clean_up()?;
    a_second_signal_ends_a_run_whose_cleanup_hangs()
}

fn run_harness_fixture(directory: &Path) -> Result<ExitCode, TerminationTestError> {
    exit_code_of(run_ending_groups_on_termination(|| {
        run_fixture_work(directory)
    })?)
}

fn run_hanging_fixture() -> Result<ExitCode, TerminationTestError> {
    exit_code_of(run_ending_groups_on_termination(run_hanging_fixture_work)?)
}

fn exit_code_of(
    outcome: RunOutcome<Result<(), TerminationTestError>>,
) -> Result<ExitCode, TerminationTestError> {
    match outcome {
        RunOutcome::Terminated(termination) => Ok(termination.exit_code()),
        RunOutcome::Finished(work) => work.map(|()| ExitCode::SUCCESS),
    }
}

fn run_hanging_fixture_work() -> Result<(), TerminationTestError> {
    let (wrapper, _child) = spawn_wrapper(WrapperEnd::WaitsForChild)?;
    println!("{READY_LINE}");
    let _wrapper_killed_on_the_first_signal = wrapper.wait()?;
    println!("{CLEANING_UP_LINE}");
    let _input_length = io::stdin()
        .read_to_end(&mut Vec::new())
        .map_err(TerminationTestError::ReadInput)?;
    Ok(())
}

fn run_fixture_work(directory: &Path) -> Result<(), TerminationTestError> {
    let _removed_on_return =
        tempfile::tempdir_in(directory).map_err(TerminationTestError::FixtureDirectory)?;
    let (exited, orphan) = spawn_wrapper(WrapperEnd::ExitsAtOnce)?;
    let (wrapper, child) = spawn_wrapper(WrapperEnd::WaitsForChild)?;
    println!(
        "{} {} {}",
        orphan.as_raw_nonzero(),
        wrapper.pid().as_raw_nonzero(),
        child.as_raw_nonzero()
    );
    let _wrapper_exit_before_the_signal = exited.wait()?;
    let _wrapper_killed_on_the_signal = wrapper.wait()?;
    let started_after_the_signal = directory.join("started-after-the-signal");
    match ScopedChild::spawn(Command::new("touch").arg(started_after_the_signal)) {
        Err(ProcessError::RefusedWhileEnding) => fs::write(directory.join(REFUSAL_RECORD), "")
            .map_err(TerminationTestError::FixtureDirectory),
        Err(error) => Err(error.into()),
        Ok(_started) => Err(TerminationTestError::StartAccepted),
    }
}

fn a_termination_signal_ends_the_groups_and_lets_the_work_clean_up()
-> Result<(), TerminationTestError> {
    let executable = std::env::current_exe().map_err(TerminationTestError::LocateExecutable)?;
    let directory = tempfile::tempdir().map_err(TerminationTestError::FixtureDirectory)?;
    let mut harness = ScopedChild::spawn(
        Command::new(executable)
            .arg(FIXTURE_ARGUMENT)
            .arg(directory.path())
            .stdout(Stdio::piped()),
    )?;
    let [orphan, wrapper, child] = read_pids(&mut harness)?;
    kill_process(harness.pid(), Signal::TERM).map_err(TerminationTestError::Signal)?;
    assert_eq!(harness.wait()?, TERMINATED);
    drop(harness);
    for started in [orphan, wrapper, child] {
        assert_eq!(
            test_kill_process(started),
            Err(Errno::SRCH),
            "process {started:?} outlived the harness"
        );
    }
    assert_eq!(
        files_in(directory.path())?,
        [OsString::from(REFUSAL_RECORD)],
        "the fixture left the temporary directory behind or started a process after the signal"
    );
    println!(
        "termination: the harness exited with {TERMINATED:?} after it removed the temporary \
         directory it created. Every started process ended. Starting a process after the \
         signal returned an error."
    );
    Ok(())
}

fn a_second_signal_ends_a_run_whose_cleanup_hangs() -> Result<(), TerminationTestError> {
    let executable = std::env::current_exe().map_err(TerminationTestError::LocateExecutable)?;
    let mut harness = ScopedChild::spawn(
        Command::new(executable)
            .arg(HANGING_FIXTURE_ARGUMENT)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped()),
    )?;
    let input = harness.take_stdin();
    let mut lines =
        BufReader::new(harness.take_stdout().ok_or(FixtureError::NoOutputPipe)?).lines();
    expect_line(&mut lines, READY_LINE)?;
    kill_process(harness.pid(), Signal::TERM).map_err(TerminationTestError::Signal)?;
    expect_line(&mut lines, CLEANING_UP_LINE)?;
    kill_process(harness.pid(), Signal::INT).map_err(TerminationTestError::Signal)?;
    drop(input);
    assert_eq!(
        harness.wait()?,
        INTERRUPTED,
        "the harness waited for its cleanup in place of exiting on the second signal"
    );
    println!("termination: the second signal ended the harness with {INTERRUPTED:?}.");
    Ok(())
}

fn expect_line(
    lines: &mut Lines<impl BufRead>,
    expected: &'static str,
) -> Result<(), TerminationTestError> {
    match lines
        .next()
        .transpose()
        .map_err(TerminationTestError::ReadFixtureOutput)?
    {
        Some(line) if line == expected => Ok(()),
        line => Err(TerminationTestError::UnexpectedFixtureLine { expected, line }),
    }
}

fn files_in(directory: &Path) -> Result<Vec<OsString>, TerminationTestError> {
    let mut names = fs::read_dir(directory)
        .and_then(|entries| {
            entries
                .map(|entry| entry.map(|entry| entry.file_name()))
                .collect::<Result<Vec<_>, _>>()
        })
        .map_err(TerminationTestError::FixtureDirectory)?;
    names.sort();
    Ok(names)
}
