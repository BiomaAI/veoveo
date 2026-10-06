"""Artifact-plane wire contracts, shared with the Rust `mcp-contract` crate.

The artifact service is the byte-level policy-enforcement point. Domain
servers never assert tenant or owner; the service stamps both from the
verified gateway identity. Asynchronous completions redeem bounded, expiring
write capabilities issued while a live identity was present.
"""

from __future__ import annotations

import uuid
from typing import Annotated, Any, Literal
from enum import Enum

from pydantic import AwareDatetime, AfterValidator, BaseModel, ConfigDict, Field, model_validator
from veoveo_mcp.types import CheckedText, ResourceUri, ResourceScheme, ResourceUriBuilder, UriAuthority, UriSegment


def _uuid_v7_str(value: str) -> str:
    parsed = uuid.UUID(value)
    if parsed.version != 7:
        raise ValueError("artifact identifiers must be UUIDv7")
    return str(parsed)


class ArtifactId(CheckedText):
    @classmethod
    def _validate(cls, value: str) -> None:
        if _uuid_v7_str(value) != value:
            raise ValueError("artifact identifiers require canonical UUIDv7 spelling")


class ArtifactTaskId(CheckedText):
    def __new__(cls, value: str):
        if not isinstance(value, str):
            raise TypeError("Artifact Task identity requires text")
        return super().__new__(cls, _uuid_v7_str(value))

    @classmethod
    def _validate(cls, value: str) -> None:
        _uuid_v7_str(value)


class ArtifactUri(ResourceUri):
    @classmethod
    def _validate(cls, value: str) -> None:
        super()._validate(value)
        parts = ResourceUri(value).components()
        if parts.query or "%" in value:
            raise ValueError("Artifact address forbids query and escapes")
        if parts.scheme == "artifact" and not parts.segments:
            ArtifactId(parts.authority)
        elif parts.authority == "artifact" and len(parts.segments) == 1:
            ArtifactId(parts.segments[0])
        else:
            raise ValueError("invalid Artifact occurrence address")

    @property
    def artifact_id(self) -> ArtifactId:
        parts = self.components()
        return ArtifactId(parts.segments[0] if parts.segments else parts.authority)

    @property
    def is_plane(self) -> bool:
        parts = self.components()
        return parts.scheme == "artifact" and not parts.segments


class ArtifactReleaseState(str, Enum):
    PRIVATE = "private"
    RELEASABLE = "releasable"
    RELEASED = "released"


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


from .identity import (
    AccessSubject, DataLabelId, TenantId, WorkContextId, PrincipalId,
    PolicyVersion, DelegationId,
)


class ArtifactProvenance(BaseModel):
    model_config = ConfigDict(frozen=True)

    producer: PrincipalId
    invocation_mode: Literal["direct", "delegated", "automated"]
    initiator: PrincipalId | None = None
    delegation_id: DelegationId | None = None
    policy_revision: PolicyVersion

    @model_validator(mode="after")
    def _admit(self):
        if self.invocation_mode == "direct":
            valid = self.initiator is not None and self.delegation_id is None
        elif self.invocation_mode == "delegated":
            valid = self.initiator is not None and self.delegation_id is not None
        else:
            valid = self.initiator is None and self.delegation_id is None
        if not valid:
            raise ValueError("Artifact provenance identities contradict invocation mode")
        return self


    def model_copy(self, *, update=None, deep=False):
        return type(self).model_validate({**self.model_dump(), **(update or {})})

class ComplianceMetadata(BaseModel):
    model_config = ConfigDict(extra="allow", frozen=True)

    classification: DataLabelId | None = None
    tenant_id: TenantId | None = None
    owner: AccessSubject | None = None
    work_context: WorkContextId | None = None
    provenance: ArtifactProvenance | None = None
    data_labels: frozenset[DataLabelId] = Field(default_factory=frozenset)
    retention_expires_at: AwareDatetime | None = None

    def model_copy(self, *, update=None, deep=False):
        return type(self).model_validate({**self.model_dump(), **(update or {})})


class ArtifactMetadata(BaseModel):
    model_config = ConfigDict(extra="allow", frozen=True)

    artifact_id: ArtifactId
    byte_len: int = Field(strict=True, ge=0, le=2**64 - 1)
    mime_type: str | None = None
    filename: str | None = None
    artifact_uri: ArtifactUri
    download_url: str | None = None
    created_at: AwareDatetime
    release_state: ArtifactReleaseState = ArtifactReleaseState.PRIVATE
    compliance: ComplianceMetadata = Field(default_factory=ComplianceMetadata)
    metadata: Any = None

    @model_validator(mode="after")
    def _admit(self):
        if self.artifact_id != self.artifact_uri.artifact_id:
            raise ValueError("Artifact metadata ID and URI identify different occurrences")
        return self

    def model_copy(self, *, update=None, deep=False):
        # Pydantic's standard copy bypasses admission of updates.
        return type(self).model_validate({**self.model_dump(), **(update or {})})

    def without_download_url(self) -> "ArtifactMetadata":
        return self.model_copy(update={"download_url": None})

    def presented_under_scheme(self, scheme: str) -> "ArtifactMetadata":
        uri = ResourceUriBuilder(ResourceScheme(scheme), UriAuthority("artifact")).segment(
            UriSegment(self.artifact_id)
        ).build()
        return self.model_copy(update={"artifact_uri": uri})


class ArtifactObject(BaseModel):
    metadata: ArtifactMetadata
    bytes_: bytes = Field(alias="bytes")

    model_config = ConfigDict(populate_by_name=True)


class PutArtifactRequest(BaseModel):
    mime_type: str | None = None
    filename: str | None = None
    classification: DataLabelId | None = None
    data_labels: set[DataLabelId] = Field(default_factory=set)
    retention_expires_at: AwareDatetime | None = None
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
    model_config = ConfigDict(extra="forbid")

    task_id: ArtifactTaskId
    expires_at: AwareDatetime
    max_artifact_count: int = Field(strict=True, gt=0, le=2**32 - 1)
    max_total_bytes: int = Field(strict=True, gt=0, le=2**64 - 1)
    required_data_labels: frozenset[DataLabelId] = Field(default_factory=frozenset, max_length=256)


class IssuedArtifactWriteCapability(BaseModel):
    capability_id: ArtifactWriteCapabilityId
    secret: ArtifactWriteCapabilitySecret
    task_id: ArtifactTaskId
    expires_at: AwareDatetime

    def __repr__(self) -> str:  # never leak the secret
        return (
            f"IssuedArtifactWriteCapability(capability_id={self.capability_id!r}, "
            f"task_id={self.task_id!r}, secret=<redacted>)"
        )


class RedeemArtifactWriteCapabilityRequest(BaseModel):
    capability_id: ArtifactWriteCapabilityId
    task_id: ArtifactTaskId
    idempotency_key: ArtifactWriteIdempotencyKey
    artifact: PutArtifactRequest

    def wire(self) -> dict[str, Any]:
        return {
            "capability_id": self.capability_id,
            "task_id": self.task_id,
            "idempotency_key": self.idempotency_key,
            "artifact": self.artifact.wire(),
        }
