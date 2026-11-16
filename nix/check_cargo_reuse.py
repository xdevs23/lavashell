# SPDX-License-Identifier: GPL-3.0-only

from pathlib import Path

from cargo_models import ArtifactIdentity, BuildFinished, CompilerArtifact, read_message
from verification_errors import (ArtifactIdentitiesChanged, BuildNotFinished, DependencyRebuilt,
                                 NoReferenceArtifacts, VerificationError)


def records(path: Path) -> list[CompilerArtifact]:
    with path.open() as lines:
        messages = [read_message(line) for line in lines]
    if not any(isinstance(message, BuildFinished) and message.success for message in messages):
        raise VerificationError(BuildNotFinished(path=path))
    return [message for message in messages if isinstance(message, CompilerArtifact)]


def verify(reference: list[CompilerArtifact], current: list[CompilerArtifact], prefix: str,
           excluded: str | None) -> int:
    def compared(artifacts: list[CompilerArtifact]) -> dict[ArtifactIdentity, CompilerArtifact]:
        return {artifact.identity: artifact for artifact in artifacts
                if artifact.package_name.startswith(prefix) and artifact.package_name != excluded}

    expected = compared(reference)
    actual = compared(current)
    if not expected:
        raise VerificationError(NoReferenceArtifacts(prefix=prefix))
    if actual.keys() != expected.keys():
        raise VerificationError(ArtifactIdentitiesChanged())
    if not all(artifact.fresh for artifact in actual.values()):
        raise VerificationError(DependencyRebuilt())
    return len(actual)
