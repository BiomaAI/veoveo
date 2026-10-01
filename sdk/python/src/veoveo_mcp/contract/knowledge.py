"""Typed ai.veoveo/knowledge-source wire models and authorized read adapters."""
from __future__ import annotations

from datetime import datetime, timezone
from enum import Enum
from hashlib import sha256
from typing import Annotated, Any, Literal, Mapping
from urllib.parse import urlsplit

import mcp.types as mcp
from pydantic import AwareDatetime, BaseModel, ConfigDict, Field, RootModel, StringConstraints, field_validator, model_validator
from pydantic.alias_generators import to_camel

from .identity import AccessSubject, DataLabelId, PrincipalId, TenantId, WorkContextId

EXTENSION_ID = "ai.veoveo/knowledge-source"
OBSERVATION_KEY = "ai.veoveo/knowledge-observation"


class WireModel(BaseModel):
    model_config = ConfigDict(extra="forbid", frozen=True, populate_by_name=True, alias_generator=to_camel)

    def wire(self) -> dict[str, Any]:
        return self.model_dump(mode="json", by_alias=True, exclude_none=True)


class _Slug(RootModel[Annotated[str, StringConstraints(strict=True, pattern=r"^[a-z][a-z0-9-]*$", max_length=128)]]):
    model_config = ConfigDict(frozen=True)

    def __str__(self) -> str:
        return self.root


class DocumentId(_Slug):
    pass


class EntityKind(_Slug):
    pass


class ExternalSystemId(_Slug):
    pass


class ExternalRecordId(RootModel[Annotated[str, StringConstraints(strict=True, min_length=1, max_length=1024)]]):
    model_config = ConfigDict(frozen=True)

    @field_validator("root")
    @classmethod
    def printable_identifier(cls, value: str) -> str:
        if not value.strip() or len(value.encode()) > 1024 or any(ord(c) < 32 or ord(c) == 127 for c in value):
            raise ValueError("external record id must be bounded and printable")
        return value


class CollectionId(RootModel[Annotated[str, StringConstraints(strict=True, pattern=r"^[a-z][a-z0-9-]{0,127}\.[a-z][a-z0-9-]{0,127}$")]]):
    model_config = ConfigDict(frozen=True)

    def __str__(self) -> str:
        return self.root


class Revision(RootModel[Annotated[str, StringConstraints(strict=True, pattern=r"^[!-~]+$", max_length=256)]]):
    model_config = ConfigDict(frozen=True)

    def __str__(self) -> str:
        return self.root


class ContentDigest(RootModel[Annotated[str, StringConstraints(strict=True, pattern=r"^[0-9a-f]{64}$")]]):
    model_config = ConfigDict(frozen=True)

    def __str__(self) -> str:
        return self.root

    @classmethod
    def of(cls, text: str) -> ContentDigest:
        return cls(sha256(text.encode("utf-8")).hexdigest())


class ImmutableFreshness(WireModel):
    immutable: Literal[True]

    @field_validator("immutable", mode="before")
    @classmethod
    def exact_true(cls, value: object) -> object:
        if value is not True:
            raise ValueError("immutable must be true")
        return value


class ExpiringFreshness(WireModel):
    max_age_seconds: Annotated[int, Field(strict=True, ge=0, le=2**32 - 1)]


class ChangeSignal(str, Enum):
    LISTEN = "listen"
    IMMUTABLE = "immutable"
    REVALIDATE = "revalidate"


class AccessModel(str, Enum):
    WORK_CONTEXT = "work-context"
    PROFILE = "profile"


class IndexingMode(str, Enum):
    CONTENT = "content"
    METADATA = "metadata"
    NONE = "none"


class CollectionDescriptor(WireModel):
    collection: CollectionId
    entity_kind: EntityKind
    enumerate: str
    freshness: ImmutableFreshness | ExpiringFreshness
    change_signal: ChangeSignal
    access: AccessModel
    indexing: IndexingMode

    @field_validator("enumerate")
    @classmethod
    def resource_template(cls, value: str) -> str:
        parsed = urlsplit(value)
        if not parsed.scheme or not parsed.netloc or parsed.username or parsed.password or parsed.fragment:
            raise ValueError("enumeration must be an absolute resource template without credentials")
        return value

    @model_validator(mode="after")
    def freshness_agrees(self) -> CollectionDescriptor:
        if isinstance(self.freshness, ImmutableFreshness) != (self.change_signal == ChangeSignal.IMMUTABLE):
            raise ValueError("immutable freshness and change signal must agree")
        return self


class AccessDescriptor(WireModel):
    tenant: TenantId
    work_context: WorkContextId
    owner: AccessSubject
    grants: tuple[AccessSubject, ...] = ()
    data_labels: tuple[DataLabelId, ...]


class ModifiedBy(WireModel):
    kind: Literal["principal"]
    id: PrincipalId


class ExternalRecord(WireModel):
    system: ExternalSystemId
    native_id: ExternalRecordId
    url: str | None = None
    mirrored_at: AwareDatetime | None = None

    @field_validator("url")
    @classmethod
    def https_url(cls, value: str | None) -> str | None:
        if value is not None:
            parsed = urlsplit(value)
            if parsed.scheme != "https" or not parsed.hostname or parsed.username or parsed.password or parsed.fragment:
                raise ValueError("external URL must be HTTPS without credentials or a fragment")
        return value


class Observation(WireModel):
    collection: CollectionId
    revision: Revision
    content_sha256: ContentDigest
    observed_at: AwareDatetime
    modified_at: AwareDatetime | None = None
    modified_by: ModifiedBy | None = None
    access: AccessDescriptor | None = None
    external: ExternalRecord | None = None
    not_modified: Annotated[bool, Field(strict=True)] = False

    def validate_collection(self, collection: CollectionDescriptor) -> None:
        if self.collection != collection.collection or (self.access is not None) != (collection.access == AccessModel.WORK_CONTEXT):
            raise ValueError("observation disagrees with collection or access model")


class ReadCondition(WireModel):
    if_none_match: Revision


def requested(capabilities: mcp.ClientCapabilities | None) -> bool:
    extensions = capabilities.extensions if capabilities else None
    if not extensions or EXTENSION_ID not in extensions:
        return False
    if extensions[EXTENSION_ID] != {}:
        raise ValueError("unsupported knowledge-source settings")
    return True


def member_result(*, uri: str, text: str, mime_type: str, observation: Observation,
                  collection: CollectionDescriptor, capabilities: mcp.ClientCapabilities | None,
                  metadata: Mapping[str, object] | None = None) -> mcp.ReadResourceResult:
    """Call only after domain authorization, including matching conditionals."""
    observation.validate_collection(collection)
    if observation.not_modified or observation.content_sha256 != ContentDigest.of(text):
        raise ValueError("member bytes disagree with source observation")
    negotiated = requested(capabilities)
    condition = ReadCondition.model_validate(metadata[EXTENSION_ID]) if negotiated and metadata and EXTENSION_ID in metadata else None
    unchanged = condition is not None and condition.if_none_match == observation.revision
    observation = observation.model_copy(update={"not_modified": unchanged})
    return mcp.ReadResourceResult(
        contents=[] if unchanged else [mcp.TextResourceContents(uri=uri, text=text, mime_type=mime_type)],
        meta={OBSERVATION_KEY: observation.wire()} if negotiated else None,
        ttl_ms=0, cache_scope="private",
    )


def docs_observation(collection: CollectionDescriptor, digest: ContentDigest) -> Observation:
    return Observation(collection=collection.collection, revision=Revision(str(digest)),
                       content_sha256=digest, observed_at=datetime.now(timezone.utc))
