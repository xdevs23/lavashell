// SPDX-License-Identifier: GPL-3.0-only
use crate::command_step::run_step;
use crate::error::{CommandStep, FileOperation, RoundTripError, SigningKeyProblem};
use lavashell_process_scope::{ProcessError, run_checked_for_stdout};
use rustix::rand::{GetRandomFlags, getrandom};
use std::fs::{self, DirBuilder, File, OpenOptions, Permissions};
use std::io::Write as _;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;
use x509_cert::Certificate;
use x509_cert::der::Decode;

pub const KEY_ALIAS: &str = "development";
const PRIVATE_DIRECTORY_MODE: u32 = 0o700;
const PRIVATE_FILE_MODE: u32 = 0o600;
const PASSWORD_BYTES: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CertificateValidity {
    Current,
    Expired,
}

impl CertificateValidity {
    fn at(not_after: SystemTime, now: SystemTime) -> Self {
        if not_after <= now {
            Self::Expired
        } else {
            Self::Current
        }
    }
}

pub struct SigningKey {
    pub keystore: PathBuf,
    pub password: PathBuf,
    directory: PathBuf,
    _lock: File,
}

impl SigningKey {
    pub fn open() -> Result<Self, RoundTripError> {
        let directory = xdg::BaseDirectories::with_prefix("lavashell/aosp-signing")
            .get_state_home()
            .ok_or(RoundTripError::HomeUnknown)?;
        let key = Self::lock_in(directory)?;
        key.create_missing_parts()?;
        Ok(key)
    }

    fn lock_in(directory: PathBuf) -> Result<Self, RoundTripError> {
        DirBuilder::new()
            .recursive(true)
            .mode(PRIVATE_DIRECTORY_MODE)
            .create(&directory)
            .map_err(RoundTripError::file(
                FileOperation::CreateDirectory,
                &directory,
            ))?;
        make_private(&directory, PRIVATE_DIRECTORY_MODE)?;
        let lock_path = directory.join("lock");
        let lock = OpenOptions::new()
            .append(true)
            .create(true)
            .mode(PRIVATE_FILE_MODE)
            .open(&lock_path)
            .map_err(RoundTripError::file(FileOperation::Lock, &lock_path))?;
        lock.lock()
            .map_err(RoundTripError::file(FileOperation::Lock, &lock_path))?;
        Ok(Self {
            keystore: directory.join("development.p12"),
            password: directory.join("password"),
            directory,
            _lock: lock,
        })
    }

    fn create_missing_parts(&self) -> Result<(), RoundTripError> {
        self.create_missing_password()?;
        let replace_keystore = match presence(&self.keystore)? {
            Presence::Missing => true,
            Presence::Present => self.certificate_validity()? == CertificateValidity::Expired,
        };
        if replace_keystore {
            self.generate_keystore()?;
        }
        make_private(&self.keystore, PRIVATE_FILE_MODE)
    }

    fn create_missing_password(&self) -> Result<(), RoundTripError> {
        match (presence(&self.keystore)?, presence(&self.password)?) {
            (Presence::Present, Presence::Missing) => {
                return Err(self.broken(SigningKeyProblem::PasswordMissing));
            }
            (Presence::Missing, Presence::Missing) => {
                write_new_password(&self.directory, &self.password)?;
            }
            (_keystore, Presence::Present) => {}
        }
        make_private(&self.password, PRIVATE_FILE_MODE)
    }

    fn certificate_validity(&self) -> Result<CertificateValidity, RoundTripError> {
        let exported = run_checked_for_stdout(
            self.keytool("-exportcert")
                .arg("-keystore")
                .arg(&self.keystore),
        )
        .map_err(|error| match error {
            ProcessError::Failed { .. } => {
                self.broken(SigningKeyProblem::CertificateUnreadable(error))
            }
            other => RoundTripError::command(CommandStep::ReadSigningKey)(other),
        })?;
        let certificate = Certificate::from_der(&exported)
            .map_err(|error| self.broken(SigningKeyProblem::CertificateMalformed(error)))?;
        let not_after = certificate
            .tbs_certificate()
            .validity()
            .not_after
            .to_system_time();
        Ok(CertificateValidity::at(not_after, SystemTime::now()))
    }

    fn generate_keystore(&self) -> Result<(), RoundTripError> {
        let temporary = tempfile::Builder::new()
            .prefix("key-")
            .tempdir_in(&self.directory)
            .map_err(RoundTripError::file(
                FileOperation::CreateTemporary,
                &self.directory,
            ))?;
        let generated = temporary.path().join("development.p12");
        run_step(
            CommandStep::CreateSigningKey,
            self.keytool("-genkeypair")
                .arg("-keystore")
                .arg(&generated)
                .arg("-keypass:file")
                .arg(&self.password)
                .args(["-keyalg", "EC", "-groupname", "secp256r1"])
                .args(["-dname", "CN=Lavashell development", "-noprompt"]),
        )?;
        fs::rename(&generated, &self.keystore)
            .map_err(RoundTripError::file(FileOperation::Rename, &self.keystore))
    }

    fn broken(&self, problem: SigningKeyProblem) -> RoundTripError {
        RoundTripError::SigningKeyBroken {
            directory: self.directory.clone(),
            problem,
        }
    }

    fn keytool(&self, operation: &str) -> Command {
        let mut command = Command::new("keytool");
        let _configured = command
            .arg(operation)
            .args(["-storetype", "PKCS12", "-alias", KEY_ALIAS])
            .arg("-storepass:file")
            .arg(&self.password);
        command
    }
}

#[derive(Clone, Copy)]
enum Presence {
    Present,
    Missing,
}

fn presence(path: &Path) -> Result<Presence, RoundTripError> {
    path.try_exists()
        .map(|exists| {
            if exists {
                Presence::Present
            } else {
                Presence::Missing
            }
        })
        .map_err(RoundTripError::file(FileOperation::CheckExistence, path))
}

fn make_private(path: &Path, mode: u32) -> Result<(), RoundTripError> {
    fs::set_permissions(path, Permissions::from_mode(mode))
        .map_err(RoundTripError::file(FileOperation::SetPermissions, path))
}

fn write_new_password(directory: &Path, path: &Path) -> Result<(), RoundTripError> {
    let mut bytes = [0_u8; PASSWORD_BYTES];
    let count =
        getrandom(&mut bytes, GetRandomFlags::empty()).map_err(RoundTripError::ReadRandomBytes)?;
    if count != bytes.len() {
        return Err(RoundTripError::RandomBytesShort { count });
    }
    let password: String = bytes
        .iter()
        .flat_map(|byte| [byte >> 4, byte & 0x0f])
        .filter_map(|nibble| char::from_digit(u32::from(nibble), 16))
        .collect();
    let mut staged = tempfile::Builder::new()
        .prefix("password-")
        .tempfile_in(directory)
        .map_err(RoundTripError::file(
            FileOperation::CreateTemporary,
            directory,
        ))?;
    staged
        .write_all(password.as_bytes())
        .map_err(RoundTripError::file(FileOperation::Write, staged.path()))?;
    staged
        .persist(path)
        .map(|_password_file| ())
        .map_err(|failure| RoundTripError::file(FileOperation::Rename, path)(failure.error))
}

#[cfg(test)]
mod tests;
