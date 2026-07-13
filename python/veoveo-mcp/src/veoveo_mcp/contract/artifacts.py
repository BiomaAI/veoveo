"""Artifact-plane wire contracts, shared with the Rust `mcp-contract` crate.

The artifact service is the byte-level policy-enforcement point. Domain
servers never assert tenant or owner; the service stamps both from the
verified gateway identity. Asynchronous completions redeem bounded, expiring
write capabilities issued while a live identity was present.
"""

from __future__ import annotations

import uuid
from datetime import datetime
from typing import Annotated, Any

from pydantic import AfterValidator, BaseModel, ConfigDict, Field


def _uuid_v7_str(value: str) -> str:
    parsed = uuid.UUID(value)
    if parsed.version != 7:
        raise ValueError("artifact identifiers must be UUIDv7")
    return str(parsed)


ArtifactId = Annotated[str, AfterValidator(_uuid_v7_str)]
ArtifactWriteCapabilityId = Annotated[str, AfterValidator(_uuid_v7_str)]


def validate_write_idempotency_key(value: str) -> str:
    if (
        not value
        or len(value) > 256
        or value.strip() != value
        or any(ch < " " or ch == "\x7f" for ch in value)
    ):
        raise ValueError(
            "artifact write idempotency key must be 1..=256 trimmed, "
            "non-control characters"
        )
    return value


ArtifactWriteIdempotencyKey = Annotated[
    str, AfterValidator(validate_write_idempotency_key)
]


def _secret(value: str) -> str:
    if len(value) < 32 or any(ch.isspace() for ch in value):
        raise ValueError(
            "artifact write capability secret must be at least 32 "
            "non-whitespace characters"
        )
    return value


ArtifactWriteCapabilitySecret = Annotated[str, AfterValidator(_secret)]


class ComplianceMetadata(BaseModel):
    model_config = ConfigDict(extra="allow")

    classification: str | None = None
    data_labels: set[str] = Field(default_factory=set)
    retention_expires_at: datetime | None = None


class ArtifactMetadata(BaseModel):
    model_config = ConfigDict(extra="allow")

    artifact_id: ArtifactId
    byte_len: int
    mime_type: str | None = None
    filename: str | None = None
    artifact_uri: str
    download_url: str | None = None
    created_at: datetime
    release_state: str = "private"
    compliance: ComplianceMetadata = Field(default_factory=ComplianceMetadata)
    metadata: Any = None

    def without_download_url(self) -> "ArtifactMetadata":
        clone = self.model_copy()
        clone.download_url = None
        return clone

    def presented_under_scheme(self, scheme: str) -> "ArtifactMetadata":
        """Rewrite `artifact_uri` into `{scheme}://artifact/{artifact_id}`."""
        clone = self.model_copy()
        clone.artifact_uri = f"{scheme}://artifact/{self.artifact_id}"
        return clone


class ArtifactObject(BaseModel):
    metadata: ArtifactMetadata
    bytes_: bytes = Field(alias="bytes")

    model_config = ConfigDict(populate_by_name=True)


class PutArtifactRequest(BaseModel):
    mime_type: str | None = None
    filename: str | None = None
    classification: str | None = None
    data_labels: set[str] = Field(default_factory=set)
    retention_expires_at: datetime | None = None
    metadata: Any = None

    def wire(self) -> dict[str, Any]:
        value: dict[str, Any] = {}
        if self.mime_type is not None:
            value["mime_type"] = self.mime_type
        if self.filename is not None:
            value["filename"] = self.filename
        if self.classification is not None:
            value["classification"] = self.classification
        if self.data_labels:
            value["data_labels"] = sorted(self.data_labels)
        if self.retention_expires_at is not None:
            value["retention_expires_at"] = self.retention_expires_at.isoformat()
        if self.metadata is not None:
            value["metadata"] = self.metadata
        return value


class IssueArtifactWriteCapabilityRequest(BaseModel):
    task_id: str
    expires_at: datetime
    max_artifact_count: int = Field(gt=0)
    max_total_bytes: int = Field(gt=0)


class IssuedArtifactWriteCapability(BaseModel):
    capability_id: ArtifactWriteCapabilityId
    secret: ArtifactWriteCapabilitySecret
    task_id: str
    expires_at: datetime

    def __repr__(self) -> str:  # never leak the secret
        return (
            f"IssuedArtifactWriteCapability(capability_id={self.capability_id!r}, "
            f"task_id={self.task_id!r}, secret=<redacted>)"
        )


class RedeemArtifactWriteCapabilityRequest(BaseModel):
    capability_id: ArtifactWriteCapabilityId
    task_id: str
    idempotency_key: ArtifactWriteIdempotencyKey
    artifact: PutArtifactRequest

    def wire(self) -> dict[str, Any]:
        return {
            "capability_id": self.capability_id,
            "task_id": self.task_id,
            "idempotency_key": self.idempotency_key,
            "artifact": self.artifact.wire(),
        }
