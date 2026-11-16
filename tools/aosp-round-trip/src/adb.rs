// SPDX-License-Identifier: GPL-3.0-only
use crate::device::DeviceEnvironment;
use crate::error::{CommandStep, RoundTripError};
use crate::run_log::RunLog;
use lavashell_process_scope::ScopedChild;
use std::net::{Ipv4Addr, TcpListener};
use std::os::fd::{BorrowedFd, OwnedFd};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub struct AdbServer {
    process: ScopedChild,
    program: PathBuf,
    port: u16,
    environment: DeviceEnvironment,
}

impl AdbServer {
    pub fn start(
        program: &Path,
        environment: DeviceEnvironment,
        log: &RunLog,
    ) -> Result<Self, RoundTripError> {
        let listener =
            TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).map_err(RoundTripError::Listen)?;
        let port = listener
            .local_addr()
            .map_err(RoundTripError::Listen)?
            .port();
        let environment = environment.with("ANDROID_ADB_SERVER_PORT", port.to_string());
        let mut command = Command::new(program);
        let _configured = environment
            .apply(&mut command)
            .args(["-L", "acceptfd:0", "server", "nodaemon"])
            .stdin(Stdio::from(OwnedFd::from(listener)))
            .stdout(log.duplicate_handle()?)
            .stderr(log.duplicate_handle()?);
        let process = ScopedChild::spawn(&mut command)
            .map_err(RoundTripError::command(CommandStep::StartAdbServer))?;
        Ok(Self {
            process,
            program: program.to_owned(),
            port,
            environment,
        })
    }

    #[must_use]
    pub fn client(&self) -> Command {
        let mut command = Command::new(&self.program);
        let _configured = self
            .environment
            .apply(&mut command)
            .args(["-P", &self.port.to_string()]);
        command
    }

    #[must_use]
    pub fn environment(&self) -> &DeviceEnvironment {
        &self.environment
    }

    #[must_use]
    pub fn exit_event(&self) -> BorrowedFd<'_> {
        self.process.exit_event()
    }
}
