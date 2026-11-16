// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

mod fixture;

use fixture::{FixtureError, WrapperEnd, parse_pids, spawn_wrapper};
use lavashell_process_scope::{
    ProcessError, ProcessExit, ScopedChild, first_ready, run_checked, run_checked_for_stdout,
};
use rustix::event::{PollFd, PollFlags, Timespec, poll};
use rustix::io::Errno;
use rustix::process::{Pid, test_kill_process};
use std::process::{Command, Stdio};

const IMMEDIATELY: Timespec = Timespec {
    tv_sec: 0,
    tv_nsec: 0,
};

fn assert_reaped(pid: Pid) {
    assert_eq!(
        test_kill_process(pid),
        Err(Errno::SRCH),
        "process {pid:?} survived its scope"
    );
}

fn waiting_for_input() -> Result<ScopedChild, FixtureError> {
    Ok(ScopedChild::spawn(
        Command::new("cat").stdin(Stdio::piped()),
    )?)
}

#[test]
fn dropping_the_scope_ends_the_wrapper_and_its_child() -> Result<(), FixtureError> {
    let (wrapper, child) = spawn_wrapper(WrapperEnd::WaitsForChild)?;
    let leader = wrapper.pid();
    drop(wrapper);
    assert_reaped(leader);
    assert_reaped(child);
    Ok(())
}

#[test]
fn an_exited_wrapper_does_not_leave_its_child_running() -> Result<(), FixtureError> {
    let (wrapper, child) = spawn_wrapper(WrapperEnd::ExitsAtOnce)?;
    assert_eq!(wrapper.wait()?, ProcessExit::Code(0));
    drop(wrapper);
    assert_reaped(child);
    Ok(())
}

#[test]
fn the_exit_event_is_ready_only_after_the_process_exited() -> Result<(), FixtureError> {
    let mut process = waiting_for_input()?;
    let mut before = [PollFd::from_borrowed_fd(
        process.exit_event(),
        PollFlags::IN,
    )];
    assert_eq!(poll(&mut before, Some(&IMMEDIATELY)), Ok(0));
    drop(process.take_stdin());
    assert_eq!(first_ready(&[(process.exit_event(), ())])?, ());
    assert_eq!(process.wait()?, ProcessExit::Code(0));
    Ok(())
}

#[test]
fn first_ready_returns_the_process_that_ended() -> Result<(), FixtureError> {
    let waiting = waiting_for_input()?;
    let finished = ScopedChild::spawn(&mut Command::new("true"))?;
    let ended = first_ready(&[
        (waiting.exit_event(), "waiting"),
        (finished.exit_event(), "finished"),
    ])?;
    assert_eq!(ended, "finished");
    Ok(())
}

#[test]
fn a_checked_command_does_not_leave_its_background_processes_running() -> Result<(), FixtureError> {
    let stdout =
        run_checked_for_stdout(Command::new("sh").args(["-c", &WrapperEnd::ExitsAtOnce.script()]))?;
    let [background] = parse_pids(String::from_utf8_lossy(&stdout).into_owned())?;
    assert_reaped(background);
    Ok(())
}

#[test]
fn a_failed_checked_command_returns_its_exit_and_its_error_output() {
    let outcome = run_checked(Command::new("sh").args(["-c", "echo missing file >&2; exit 3"]));
    assert!(
        matches!(
            &outcome,
            Err(ProcessError::Failed { exit: ProcessExit::Code(3), stderr, .. })
                if stderr == "missing file\n"
        ),
        "{outcome:?}"
    );
}

#[test]
fn a_checked_command_writes_its_standard_output_to_the_null_device() -> Result<(), FixtureError> {
    run_checked(Command::new("sh").args(["-c", r#"[ "$(readlink /proc/$$/fd/1)" = /dev/null ]"#]))?;
    Ok(())
}
