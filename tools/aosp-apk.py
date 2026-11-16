#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only

import argparse
from datetime import UTC, datetime
import fcntl
import os
from pathlib import Path
import secrets
import tempfile

from cryptography.hazmat.primitives.serialization import pkcs12

from next_steps import NextStep, ToolError, exit_with_next_step
from process_scope import run_checked

PRIVATE_FILE_MODE = 0o600
BYTES_PER_KIB = 1024


class SigningKeyBroken(ToolError):
    def __init__(self, problem: str) -> None:
        super().__init__(f"the development signing key cannot be used: {problem}", NextStep.REPLACE_SIGNING_KEY)


def private_file_opener(path: str, flags: int) -> int:
    return os.open(path, flags, PRIVATE_FILE_MODE)


def signing_key() -> tuple[Path, Path]:
    state = Path(os.environ.get("XDG_STATE_HOME", Path.home() / ".local/state"))
    directory = state / "lavashell/aosp-signing"
    directory.mkdir(mode=0o700, parents=True, exist_ok=True)
    directory.chmod(0o700)
    with open(directory / "lock", "a", opener=private_file_opener) as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        return create_signing_key(directory)


def expired(keystore: Path, password: Path) -> bool:
    entry = pkcs12.load_pkcs12(keystore.read_bytes(), password.read_bytes()).cert
    if entry is None:
        raise SigningKeyBroken("its keystore contains no certificate")
    return entry.certificate.not_valid_after_utc <= datetime.now(UTC)


def create_signing_key(directory: Path) -> tuple[Path, Path]:
    password = directory / "password"
    keystore = directory / "development.p12"
    if keystore.exists() and not password.exists():
        raise SigningKeyBroken("its password file is missing")
    if not password.exists():
        with open(password, "x", opener=private_file_opener) as output:
            output.write(secrets.token_urlsafe())
    password.chmod(PRIVATE_FILE_MODE)
    if not keystore.exists() or expired(keystore, password):
        previous = os.umask(0o077)
        try:
            with tempfile.TemporaryDirectory(prefix="key-", dir=directory) as temporary:
                generated = Path(temporary) / "development.p12"
                run_checked([
                    "keytool", "-genkeypair", "-keystore", str(generated),
                    "-storetype", "PKCS12", "-storepass:file", str(password),
                    "-keypass:file", str(password), "-alias", "development",
                    "-keyalg", "EC", "-groupname", "secp256r1",
                    "-dname", "CN=Lavashell development", "-noprompt",
                ])
                os.replace(generated, keystore)
        finally:
            os.umask(previous)
    keystore.chmod(PRIVATE_FILE_MODE)
    return keystore, password


def page_size_argument(text: str) -> int:
    page_size = int(text)
    if page_size % BYTES_PER_KIB:
        raise argparse.ArgumentTypeError(f"{text} is not a whole number of KiB")
    return page_size


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--sdk", required=True, type=Path)
    parser.add_argument("--build-tools", required=True)
    parser.add_argument("--page-size", required=True, type=page_size_argument, help="Page size in bytes")
    parser.add_argument("--alignment", required=True, type=int, help="Alignment in bytes of the files")
    parser.add_argument("--apk", required=True, type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    build_tools = args.sdk / "build-tools" / args.build_tools
    signer = str(build_tools / "apksigner")
    page_size_kib = args.page_size // BYTES_PER_KIB
    aligned = [str(build_tools / "zipalign"), "-c", "-P", str(page_size_kib), str(args.alignment)]
    run_checked([*aligned, str(args.apk)])
    keystore, password = signing_key()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="aosp-sign-", dir=args.output.parent) as temporary:
        signed = Path(temporary) / "signed.apk"
        run_checked([
            signer, "sign", "--ks", str(keystore), "--ks-key-alias", "development",
            "--ks-pass", f"file:{password}",
            "--v4-signing-enabled", "false", "--out", str(signed), str(args.apk),
        ])
        run_checked([signer, "verify", str(signed)])
        run_checked([*aligned, str(signed)])
        os.replace(signed, args.output)
    print(args.output.resolve())


if __name__ == "__main__":
    try:
        main()
    except ToolError as error:
        exit_with_next_step(error)
