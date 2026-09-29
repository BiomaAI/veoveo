"""Typed Datasheet routes built and parsed with the standard URI library."""
from __future__ import annotations

import re
import uuid
from dataclasses import dataclass
from urllib.parse import parse_qsl, quote, unquote, urlencode, urlsplit, urlunsplit

from pydantic import TypeAdapter
from veoveo_mcp.contract.artifacts import ArtifactId
from veoveo_mcp.tasks import parse_task_id

from .catalog import ReportCursor, UsageCursor

SCHEME = "datasheet"
REPORTS_URI = "datasheet://reports"
REPORTS_TEMPLATE = "datasheet://reports{?cursor}"
WORKBENCH_APP_URI = "ui://datasheet/workbench.html"
USAGE_ROOT_URI = "datasheet://usage"
USAGE_TEMPLATE = "datasheet://usage{?cursor}"
USAGE_TASK_TEMPLATE = "datasheet://usage/task/{task_id}"
ARTIFACT_TEMPLATE = "datasheet://artifact/{artifact_id}"
DOCS_URI = "datasheet://docs"
CONTRACT_URI = "datasheet://contract"
_artifact_id = TypeAdapter(ArtifactId)


def _uri(authority: str, segments: tuple[str, ...] = (), cursor: str | None = None) -> str:
    path = "/" + "/".join(quote(segment, safe="") for segment in segments) if segments else ""
    query = urlencode({"cursor": cursor}) if cursor is not None else ""
    return urlunsplit((SCHEME, authority, path, query, ""))


def _parts(uri: str):
    if any(ord(char) < 33 or ord(char) > 126 for char in uri):
        raise ValueError("resource URI must use printable ASCII with encoded components")
    value = urlsplit(uri)
    if value.scheme != SCHEME:
        return None
    if "#" in uri or value.username or value.password or value.port is not None:
        raise ValueError("unsupported Datasheet URI authority or fragment")
    pairs = parse_qsl(value.query, keep_blank_values=True, strict_parsing=True)
    if len(pairs) != len(dict(pairs)) or any(key != "cursor" for key, _ in pairs):
        raise ValueError("unsupported or repeated resource query parameter")
    segments = tuple(unquote(part, errors="strict") for part in value.path.split("/")[1:])
    return value.netloc, segments, dict(pairs)


@dataclass(frozen=True)
class ReportCatalogResource:
    after: ReportCursor | None = None

    def uri(self) -> str:
        return _uri("reports", cursor=self.after.encode() if self.after else None)


@dataclass(frozen=True)
class UsageCatalogResource:
    after: UsageCursor | None = None

    def uri(self) -> str:
        return _uri("usage", cursor=self.after.encode() if self.after else None)

def usage_task_uri(task_id: uuid.UUID) -> str:
    if not isinstance(task_id, uuid.UUID):
        raise TypeError("usage addresses require a parsed Task UUID")
    return _uri("usage", ("task", str(parse_task_id(task_id))))


def artifact_uri(artifact_id: ArtifactId) -> str:
    return _uri("artifact", (_artifact_id.validate_python(artifact_id),))

def doc_uri(doc_id: str) -> str:
    if re.fullmatch(r"[a-z][a-z0-9-]{0,63}", doc_id) is None:
        raise ValueError("invalid document id")
    return _uri("docs", (doc_id,))

@dataclass(frozen=True)
class TaskUsageResource:
    task_id: uuid.UUID


@dataclass(frozen=True)
class ArtifactResource:
    artifact_id: ArtifactId


@dataclass(frozen=True)
class DocumentResource:
    document_id: str


ResourceAddress = ReportCatalogResource | UsageCatalogResource | TaskUsageResource | ArtifactResource | DocumentResource


def parse_resource_uri(uri: str) -> ResourceAddress | None:
    parts = _parts(uri)
    if parts is None:
        return None
    authority, segments, query = parts
    if not segments and authority in {"reports", "usage"}:
        cursor = query.get("cursor")
        if authority == "reports":
            return ReportCatalogResource(ReportCursor.decode(cursor) if cursor is not None else None)
        return UsageCatalogResource(UsageCursor.decode(cursor) if cursor is not None else None)
    if query:
        raise ValueError("this resource does not accept query parameters")
    match authority, segments:
        case "usage", ("task", value):
            task_id = parse_task_id(value)
            if str(task_id) != value:
                raise ValueError("Task URI requires canonical UUIDv7 spelling")
            return TaskUsageResource(task_id)
        case "artifact", (value,):
            return ArtifactResource(_artifact_id.validate_python(value))
        case "docs", (value,):
            doc_uri(value)
            return DocumentResource(value)
        case _:
            return None
