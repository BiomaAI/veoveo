"""Datasheet's closed resource vocabulary and typed component builders."""
from __future__ import annotations

import uuid
from dataclasses import dataclass
from enum import Enum

from veoveo_mcp.contract.artifacts import ArtifactId
from veoveo_mcp.tasks import parse_task_id
from veoveo_mcp.types import (
    ResourceScheme, ResourceTemplateUri, ResourceUri, ResourceUriBuilder,
    ScopeEnum, UriAuthority, UriSegment,
)

from .catalog import ReportCursor, UsageCursor

SCHEME = ResourceScheme("datasheet")
REPORTS_URI = ResourceUri("datasheet://reports")
REPORTS_TEMPLATE = ResourceTemplateUri("datasheet://reports{?cursor}")
WORKBENCH_APP_URI = ResourceUri("ui://datasheet/workbench.html")
USAGE_ROOT_URI = ResourceUri("datasheet://usage")
USAGE_TEMPLATE = ResourceTemplateUri("datasheet://usage{?cursor}")
USAGE_TASK_TEMPLATE = ResourceTemplateUri("datasheet://usage/task/{task_id}")
ARTIFACT_TEMPLATE = ResourceTemplateUri("datasheet://artifact/{artifact_id}")
DOCS_URI = ResourceUri("datasheet://docs")
DOCS_TEMPLATE = ResourceTemplateUri("datasheet://docs/{doc_id}")
CONTRACT_URI = ResourceUri("datasheet://contract")


class DatasheetScope(ScopeEnum):
    """Datasheet defines no domain scopes; admission uses caller ownership."""


class DocumentId(Enum):
    AGENTS = "agents"
    DESIGN = "design"


class FixedResource(Enum):
    CONTRACT = CONTRACT_URI
    WORKBENCH = WORKBENCH_APP_URI

    def to_uri(self) -> ResourceUri:
        return self.value


def _builder(authority: str) -> ResourceUriBuilder:
    return ResourceUriBuilder(SCHEME, UriAuthority(authority))


@dataclass(frozen=True)
class ReportCatalogResource:
    after: ReportCursor | None = None

    def __post_init__(self) -> None:
        if self.after is not None and not isinstance(self.after, ReportCursor):
            raise TypeError("report addresses require ReportCursor")

    def to_uri(self) -> ResourceUri:
        builder = _builder("reports")
        return (builder.query_pair("cursor", self.after.encode()) if self.after else builder).build()


@dataclass(frozen=True)
class UsageCatalogResource:
    after: UsageCursor | None = None

    def __post_init__(self) -> None:
        if self.after is not None and not isinstance(self.after, UsageCursor):
            raise TypeError("usage addresses require UsageCursor")

    def to_uri(self) -> ResourceUri:
        builder = _builder("usage")
        return (builder.query_pair("cursor", self.after.encode()) if self.after else builder).build()


@dataclass(frozen=True)
class DocumentCatalogResource:
    after: DocumentId | None = None

    def __post_init__(self) -> None:
        if self.after is not None and not isinstance(self.after, DocumentId):
            raise TypeError("document addresses require DocumentId")

    def to_uri(self) -> ResourceUri:
        builder = _builder("docs")
        return (builder.query_pair("cursor", self.after.value) if self.after else builder).build()


@dataclass(frozen=True)
class TaskUsageResource:
    task_id: uuid.UUID

    def __post_init__(self) -> None:
        if not isinstance(self.task_id, uuid.UUID):
            raise TypeError("usage addresses require a parsed Task UUID")
        parse_task_id(self.task_id)

    def to_uri(self) -> ResourceUri:
        return _builder("usage").segment(UriSegment("task")).segment(UriSegment(str(self.task_id))).build()


@dataclass(frozen=True)
class ArtifactResource:
    artifact_id: ArtifactId

    def __post_init__(self) -> None:
        if not isinstance(self.artifact_id, ArtifactId):
            raise TypeError("artifact addresses require ArtifactId")

    def to_uri(self) -> ResourceUri:
        return _builder("artifact").segment(UriSegment(self.artifact_id)).build()


@dataclass(frozen=True)
class DocumentResource:
    document_id: DocumentId

    def __post_init__(self) -> None:
        if not isinstance(self.document_id, DocumentId):
            raise TypeError("document addresses require DocumentId")

    def to_uri(self) -> ResourceUri:
        return _builder("docs").segment(UriSegment(self.document_id.value)).build()


DatasheetResource = (FixedResource | ReportCatalogResource | UsageCatalogResource |
                     DocumentCatalogResource | TaskUsageResource | ArtifactResource | DocumentResource)


def usage_task_uri(task_id: uuid.UUID) -> ResourceUri:
    return TaskUsageResource(task_id).to_uri()


def artifact_uri(artifact_id: ArtifactId) -> ResourceUri:
    return ArtifactResource(artifact_id).to_uri()


def parse_resource_uri(uri: ResourceUri) -> DatasheetResource | None:
    if not isinstance(uri, ResourceUri):
        raise TypeError("parse resource text into ResourceUri at ingress")
    if uri in {CONTRACT_URI, WORKBENCH_APP_URI}:
        return FixedResource(uri)
    parts = uri.components()
    if parts.scheme != SCHEME:
        return None
    query = dict(parts.query)
    if set(query) - {"cursor"}:
        raise ValueError("unsupported resource query parameter")
    resource: DatasheetResource
    if not parts.segments and parts.authority in {"reports", "usage", "docs"}:
        cursor = query.get("cursor")
        match parts.authority:
            case "reports":
                resource = ReportCatalogResource(ReportCursor.decode(cursor) if cursor is not None else None)
            case "usage":
                resource = UsageCatalogResource(UsageCursor.decode(cursor) if cursor is not None else None)
            case _:
                resource = DocumentCatalogResource(DocumentId(cursor) if cursor is not None else None)
    else:
        if query:
            raise ValueError("this resource does not accept query parameters")
        match parts.authority, parts.segments:
            case "usage", ("task", value):
                resource = TaskUsageResource(parse_task_id(value))
            case "artifact", (value,):
                resource = ArtifactResource(ArtifactId(value))
            case "docs", (value,):
                resource = DocumentResource(DocumentId(value))
            case _:
                return None
    if resource.to_uri() != uri:
        raise ValueError("resource requires its canonical owner spelling")
    return resource
