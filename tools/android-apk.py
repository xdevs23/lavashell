#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Sign an APK locally with a development key."""

import argparse
from datetime import UTC, datetime
import fcntl
import os
from pathlib import Path
import secrets
import subprocess
import tempfile

from cryptography.hazmat.primitives.serialization import pkcs12


def run(*command):
    subprocess.run(command, check=True)


def signing_key():
    state = Path(os.environ.get("XDG_STATE_HOME", Path.home() / ".local/state"))
    directory = state / "lavashell/android-signing"
    directory.mkdir(mode=0o700, parents=True, exist_ok=True)
    directory.chmod(0o700)
    with open(directory / "lock", "a", opener=lambda path, flags: os.open(path, flags, 0o600)) as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        return create_signing_key(directory)


def expired(keystore, password):
    """Whether the certificate of the development key is no longer valid."""
    certificate = pkcs12.load_pkcs12(keystore.read_bytes(), password.read_bytes()).cert.certificate
    return certificate.not_valid_after_utc <= datetime.now(UTC)


def create_signing_key(directory):
    password = directory / "password"
    keystore = directory / "development.p12"
    if keystore.exists() and not password.exists():
        raise FileNotFoundError("the password file for the development signing key is missing")
    if not password.exists():
        with open(password, "x", opener=lambda path, flags: os.open(path, flags, 0o600)) as output:
            output.write(secrets.token_urlsafe())
    password.chmod(0o600)
    if not keystore.exists() or expired(keystore, password):
        previous = os.umask(0o077)
        try:
            with tempfile.TemporaryDirectory(prefix="key-", dir=directory) as temporary:
                generated = Path(temporary) / "development.p12"
                run(
                    "keytool", "-genkeypair", "-keystore", str(generated),
                    "-storetype", "PKCS12", "-storepass:file", str(password),
                    "-keypass:file", str(password), "-alias", "development",
                    "-keyalg", "EC", "-groupname", "secp256r1",
                    "-dname", "CN=Lavashell development", "-noprompt",
                )
                os.replace(generated, keystore)
        finally:
            os.umask(previous)
    keystore.chmod(0o600)
    return keystore, password


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sdk", required=True, type=Path)
    parser.add_argument("--build-tools", required=True)
    parser.add_argument("--page-size-kib", required=True, type=int)
    parser.add_argument("--apk", required=True, type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    build_tools = args.sdk / "build-tools" / args.build_tools
    signer = str(build_tools / "apksigner")
    aligned = [str(build_tools / "zipalign"), "-c", "-P", str(args.page_size_kib), "4"]
    run(*aligned, str(args.apk))
    keystore, password = signing_key()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="android-sign-", dir=args.output.parent) as temporary:
        signed = Path(temporary) / "signed.apk"
        run(
            signer, "sign", "--ks", str(keystore), "--ks-key-alias", "development",
            "--ks-pass", f"file:{password}",
            "--v4-signing-enabled", "false", "--out", str(signed), str(args.apk),
        )
        run(signer, "verify", str(signed))
        run(*aligned, str(signed))
        os.replace(signed, args.output)
    print(args.output.resolve())


if __name__ == "__main__":
    main()
