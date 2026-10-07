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

from pydantic.alias_generators import to_camel
from .wire import CurrentWireModel
from pydantic import AfterValidator, BaseModel, ConfigDict, Field, model_validator
from veoveo_mcp.types import CheckedText, ChronoTimestamp, ResourceUri, ResourceScheme, ResourceUriBuilder, UriAuthority, UriSegment


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


class ArtifactUploadId(CheckedText):
    @classmethod
    def _validate(cls, value: str) -> None:
        if _uuid_v7_str(value) != value:
            raise ValueError("Artifact upload identifiers require canonical UUIDv7 spelling")


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


class ArtifactProvenance(CurrentWireModel):
    model_config = ConfigDict(alias_generator=to_camel, validate_by_name=False, serialize_by_alias=True, extra="forbid", frozen=True)

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
        return type(self).model_validate({**self.model_dump(), **{to_camel(key): value for key, value in (update or {}).items()}})

class ComplianceMetadata(CurrentWireModel):
    model_config = ConfigDict(alias_generator=to_camel, validate_by_name=False, serialize_by_alias=True, extra="forbid", frozen=True)

    classification: DataLabelId | None = None
    tenant_id: TenantId | None = None
    owner: AccessSubject | None = None
    work_context: WorkContextId | None = None
    provenance: ArtifactProvenance | None = None
    data_labels: frozenset[DataLabelId] = Field(default_factory=frozenset)
    retention_expires_at: ChronoTimestamp | None = None

    def model_copy(self, *, update=None, deep=False):
        return type(self).model_validate({**self.model_dump(), **{to_camel(key): value for key, value in (update or {}).items()}})


class ArtifactMetadata(CurrentWireModel):
    model_config = ConfigDict(alias_generator=to_camel, validate_by_name=False, serialize_by_alias=True, extra="forbid", frozen=True)

    artifact_id: ArtifactId
    byte_len: int = Field(strict=True, ge=0, le=2**64 - 1)
    mime_type: str | None = None
    filename: str | None = None
    artifact_uri: ArtifactUri
    download_url: str | None = None
    created_at: ChronoTimestamp
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
        return type(self).model_validate({**self.model_dump(), **{to_camel(key): value for key, value in (update or {}).items()}})

    def without_download_url(self) -> "ArtifactMetadata":
        return self.model_copy(update={"download_url": None})

    def presented_under_scheme(self, scheme: str) -> "ArtifactMetadata":
        uri = ResourceUriBuilder(ResourceScheme(scheme), UriAuthority("artifact")).segment(
            UriSegment(self.artifact_id)
        ).build()
        return self.model_copy(update={"artifact_uri": uri})


class ArtifactUploadReceipt(CurrentWireModel):
    """Complete current Artifact owner upload receipt."""
    model_config = ConfigDict(alias_generator=to_camel, validate_by_name=False, serialize_by_alias=True, extra="forbid", frozen=True)

    upload_id: ArtifactUploadId
    artifact_id: ArtifactId
    artifact_uri: ArtifactUri
    sha256: str = Field(strict=True, min_length=64, max_length=64, pattern=r"^[0-9a-f]{64}$")
    byte_len: int = Field(strict=True, ge=0, le=2**64 - 1)
    mime_type: str = Field(strict=True)
    filename: str = Field(strict=True)
    created_at: ChronoTimestamp

    @model_validator(mode="after")
    def _admit(self):
        if not self.artifact_uri.is_plane or self.artifact_uri.artifact_id != self.artifact_id:
            raise ValueError("upload receipt identifies a different plane occurrence")
        return self

    def model_copy(self, *, update=None, deep=False):
        return type(self).model_validate({**self.model_dump(), **{to_camel(key): value for key, value in (update or {}).items()}})


class ArtifactObject(BaseModel):
    metadata: ArtifactMetadata
    bytes_: bytes = Field(alias="bytes")

    model_config = ConfigDict(populate_by_name=True)


class PutArtifactRequest(CurrentWireModel):
    model_config = ConfigDict(alias_generator=to_camel, validate_by_name=False, serialize_by_alias=True, extra="forbid")

    mime_type: str | None = None
    filename: str | None = None
    classification: DataLabelId | None = None
    data_labels: set[DataLabelId] = Field(default_factory=set)
    retention_expires_at: ChronoTimestamp | None = None
    metadata: Any = None

    def wire(self) -> dict[str, Any]:
        value: dict[str, Any] = {}
        if self.mime_type is not None:
            value["mimeType"] = self.mime_type
        if self.filename is not None:
            value["filename"] = self.filename
        if self.classification is not None:
            value["classification"] = self.classification
        if self.data_labels:
            value["dataLabels"] = sorted(self.data_labels)
        if self.retention_expires_at is not None:
            value["retentionExpiresAt"] = ChronoTimestamp(str(self.retention_expires_at)).wire
        if self.metadata is not None:
            value["metadata"] = self.metadata
        return value

    def model_copy(self, *, update=None, deep=False):
        return type(self).model_validate({**self.model_dump(), **{to_camel(key): value for key, value in (update or {}).items()}})


class IssueArtifactWriteCapabilityRequest(CurrentWireModel):
    model_config = ConfigDict(alias_generator=to_camel, validate_by_name=False, serialize_by_alias=True, extra="forbid")

    task_id: ArtifactTaskId
    expires_at: ChronoTimestamp
    max_artifact_count: int = Field(strict=True, gt=0, le=2**32 - 1)
    max_total_bytes: int = Field(strict=True, gt=0, le=2**64 - 1)
    required_data_labels: frozenset[DataLabelId] = Field(default_factory=frozenset, max_length=256)

    def model_copy(self, *, update=None, deep=False):
        return type(self).model_validate({**self.model_dump(), **{to_camel(key): value for key, value in (update or {}).items()}})


class IssuedArtifactWriteCapability(CurrentWireModel):
    model_config = ConfigDict(alias_generator=to_camel, validate_by_name=False, serialize_by_alias=True, extra="forbid")

    capability_id: ArtifactWriteCapabilityId
    secret: ArtifactWriteCapabilitySecret
    task_id: ArtifactTaskId
    expires_at: ChronoTimestamp

    def __repr__(self) -> str:  # never leak the secret
        return (
            f"IssuedArtifactWriteCapability(capability_id={self.capability_id!r}, "
            f"task_id={self.task_id!r}, secret=<redacted>)"
        )

    def model_copy(self, *, update=None, deep=False):
        return type(self).model_validate({**self.model_dump(), **{to_camel(key): value for key, value in (update or {}).items()}})


class RedeemArtifactWriteCapabilityRequest(CurrentWireModel):
    model_config = ConfigDict(alias_generator=to_camel, validate_by_name=False, serialize_by_alias=True, extra="forbid")

    capability_id: ArtifactWriteCapabilityId
    task_id: ArtifactTaskId
    idempotency_key: ArtifactWriteIdempotencyKey
    artifact: PutArtifactRequest

    def wire(self) -> dict[str, Any]:
        return {
            "capabilityId": self.capability_id,
            "taskId": self.task_id,
            "idempotencyKey": self.idempotency_key,
            "artifact": self.artifact.wire(),
        }
