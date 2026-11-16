// SPDX-License-Identifier: GPL-3.0-only
use super::{CertificateValidity, PRIVATE_FILE_MODE, SigningKey};
use crate::error::{RoundTripError, SigningKeyProblem};
use std::fs::{self, Permissions};
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::time::{Duration, SystemTime};

#[derive(Debug, thiserror::Error)]
enum TestError {
    #[error(transparent)]
    RoundTrip(#[from] RoundTripError),
    #[error(transparent)]
    Io(#[from] io::Error),
}

const PERMISSION_BITS: u32 = 0o777;

#[test]
fn refuses_a_keystore_whose_password_file_is_missing() -> Result<(), TestError> {
    let directory = tempfile::tempdir()?;
    let key = SigningKey::lock_in(directory.path().to_owned())?;
    fs::write(&key.keystore, "")?;
    assert!(matches!(
        key.create_missing_parts(),
        Err(RoundTripError::SigningKeyBroken {
            problem: SigningKeyProblem::PasswordMissing,
            ..
        })
    ));
    assert!(!key.password.try_exists()?);
    Ok(())
}

#[test]
fn leaves_an_existing_password_unchanged_and_makes_it_private() -> Result<(), TestError> {
    let directory = tempfile::tempdir()?;
    let key = SigningKey::lock_in(directory.path().to_owned())?;
    fs::write(&key.password, "existing password")?;
    fs::set_permissions(&key.password, Permissions::from_mode(0o644))?;
    key.create_missing_password()?;
    assert_eq!(fs::read_to_string(&key.password)?, "existing password");
    assert_eq!(
        fs::metadata(&key.password)?.permissions().mode() & PERMISSION_BITS,
        PRIVATE_FILE_MODE
    );
    Ok(())
}

#[test]
fn a_certificate_expires_at_the_end_of_its_validity() {
    let not_after = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
    let second = Duration::from_secs(1);
    assert_eq!(
        CertificateValidity::at(not_after, not_after - second),
        CertificateValidity::Current
    );
    assert_eq!(
        CertificateValidity::at(not_after, not_after),
        CertificateValidity::Expired
    );
    assert_eq!(
        CertificateValidity::at(not_after, not_after + second),
        CertificateValidity::Expired
    );
}
