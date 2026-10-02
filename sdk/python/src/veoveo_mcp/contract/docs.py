"""Embedded server documents and the contract self-declaration.

Python equivalent of the Rust `veoveo_mcp_contract::docs` module: it
implements the Well-Known Surface of `mcp/contract/DESIGN.md` (C18-C21) —
documents embedded in the server package, the machine-readable contract
declaration served at `{scheme}://contract`, and llms.txt rendering for the
administrative mount. Servers obtain the document set with
:func:`server_docs` at import time, so the deployed package serves the manual
of exactly the version it was built from and fails closed when a document is
missing or empty.
"""

from __future__ import annotations

from dataclasses import dataclass
from enum import Enum
from importlib.resources import files
from pathlib import Path
from typing import Any, Iterator, Mapping, NotRequired, TypedDict
import re
import json
import mcp.types as types
from rfc3986 import URIReference
from veoveo_mcp.types import ResourceScheme, ResourceTemplateUri, ResourceUri, ResourceUriBuilder, UriAuthority, UriSegment

from .knowledge import (AccessModel, ChangeSignal, CollectionDescriptor, CollectionId,
    ContentDigest, EntityKind, ImmutableFreshness, IndexingMode, docs_observation, member_result)

CONTRACT_REVISION = 3
"""The normative contract revision this package implements."""

DOC_ID_AGENTS = "agents"
"""Identifier of the required agent manual document."""

DOC_ID_DESIGN = "design"
"""Identifier of the required domain design document."""

DOC_TITLE_AGENTS = "Agent work manual"
DOC_TITLE_DESIGN = "Domain design"

REQUIRED_AGENT_SECTIONS: tuple[str, ...] = (
    "## Purpose",
    "## Invariants",
    "## Build And Test",
    "## Contract Compliance",
)
"""Section headers every server `AGENTS.md` must contain (C23)."""

CHECKLIST_IDS: tuple[str, ...] = (
    "C01", "C02", "C03", "C04", "C05", "C06", "C07", "C08", "C09", "C10",
    "C11", "C12", "C13", "C14", "C15", "C16", "C17", "C18", "C19", "C20",
    "C21", "C22", "C23", "C24", "C25", "C26", "C27", "C28", "C29", "C30",
    "C31", "C32",
)
"""Stable identifiers of the compliance checklist in `DESIGN.md`."""


class ServerDocsError(ValueError):
    """A server document set could not be assembled fail-closed."""


class DocumentIndexEntry(TypedDict):
    id: str
    title: str
    uri: str


class DocumentPage(TypedDict):
    items: list[DocumentIndexEntry]
    nextCursor: NotRequired[str]


@dataclass(frozen=True)
class ServerDoc:
    """One document embedded from the server package."""

    id: str
    title: str
    body: str
    digest: ContentDigest | None = None

    def __post_init__(self) -> None:
        if not re.fullmatch(r"[a-z][a-z0-9-]{0,127}", self.id):
            raise ServerDocsError("invalid server document id")
        if not self.title.strip():
            raise ServerDocsError(f"server document `{self.id}` title must be non-empty")
        if not self.body.strip():
            raise ServerDocsError(f"server document `{self.id}` body must be non-empty")
        actual = ContentDigest.of(self.body)
        if self.digest is not None and self.digest != actual:
            raise ServerDocsError("embedded document digest mismatch; rebuild the package")
        object.__setattr__(self, "digest", actual)

    def wire(self) -> dict[str, str]:
        """The index entry shape, matching the Rust `ServerDoc` serialization
        (the body is never serialized into the index)."""
        return {"id": self.id, "title": self.title}


@dataclass(frozen=True)
class ServerDocs:
    """The embedded document set a server serves under `{scheme}://docs`."""

    server: str
    docs: tuple[ServerDoc, ...]

    def __post_init__(self) -> None:
        if not self.server.strip():
            raise ServerDocsError("server name must be non-empty")

    def doc(self, doc_id: str) -> ServerDoc | None:
        for doc in self.docs:
            if doc.id == doc_id:
                return doc
        return None

    def __iter__(self) -> Iterator[ServerDoc]:
        return iter(self.docs)

    def collection(self, scheme: str | None = None) -> CollectionDescriptor:
        return CollectionDescriptor(
            collection=CollectionId(f"{self.server}.docs"), entity_kind=EntityKind("document"),
            enumerate=ResourceUriBuilder(ResourceScheme(scheme or self.server), UriAuthority("docs")).build(),
            freshness=ImmutableFreshness(immutable=True), change_signal=ChangeSignal.IMMUTABLE,
            access=AccessModel.PROFILE, indexing=IndexingMode.CONTENT,
        )

    def knowledge_template(self, scheme: str | None = None) -> types.ResourceTemplate:
        collection = self.collection(scheme)
        template = ResourceTemplateUri(URIReference(ResourceScheme(scheme or self.server), "docs", "/{doc_id}", None, None).unsplit())
        return types.ResourceTemplate(uri_template=template,
            name="documents", title="Server documentation", mime_type="text/markdown",
            meta={"ai.veoveo/knowledge-source": collection.wire()})

    def read_authorized(self, uri: str, *, capabilities: types.ClientCapabilities | None = None,
                        metadata: Mapping[str, object] | None = None,
                        scheme: str | None = None) -> types.ReadResourceResult | None:
        """Read docs after the server has authenticated and admitted this caller."""
        parts = ResourceUri(uri).components()
        if parts.scheme != (scheme or self.server) or parts.authority != "docs":
            return None
        builder = ResourceUriBuilder(parts.scheme, parts.authority)
        if not parts.segments:
            query = dict(parts.query)
            if set(query) - {"cursor"}:
                raise ServerDocsError("invalid documentation cursor")
            cursor = query.get("cursor")
            if cursor is not None:
                builder = builder.query_pair("cursor", cursor)
            if builder.build() != uri:
                raise ServerDocsError("noncanonical documentation URI")
            return types.ReadResourceResult(contents=[types.TextResourceContents(uri=uri,
                text=json.dumps(self.index_wire(cursor, scheme=scheme)), mime_type="application/json")],
                ttl_ms=0, cache_scope="private")
        if parts.query or len(parts.segments) != 1:
            raise ServerDocsError("invalid document member address")
        doc = self.doc(parts.segments[0])
        if doc is None:
            raise ServerDocsError("unknown server document")
        if builder.segment(UriSegment(doc.id)).build() != uri:
            raise ServerDocsError("noncanonical document member URI")
        assert doc.digest is not None
        collection = self.collection(scheme)
        return member_result(uri=uri, text=doc.body, mime_type="text/markdown",
            observation=docs_observation(collection, doc.digest), collection=collection,
            capabilities=capabilities, metadata=metadata)

    def index_wire(self, cursor: str | None = None, *, scheme: str | None = None) -> DocumentPage:
        """Stable pages of at most 32 document identities and concrete URIs."""
        scheme = scheme or self.server
        if not re.fullmatch(r"[a-z][a-z0-9+.-]*", scheme):
            raise ServerDocsError("invalid document URI scheme")
        ordered = sorted(self.docs, key=lambda doc: doc.id)
        ids = [doc.id for doc in ordered]
        if len(ids) != len(set(ids)):
            raise ServerDocsError("duplicate document id")
        if cursor is not None and cursor not in ids:
            raise ServerDocsError("unknown document cursor")
        start = 0 if cursor is None else ids.index(cursor) + 1
        end = min(start + 32, len(ordered))
        page: DocumentPage = {"items": [
            {"id": doc.id, "title": doc.title,
             "uri": ResourceUriBuilder(ResourceScheme(scheme), UriAuthority("docs")).segment(UriSegment(doc.id)).build()}
            for doc in ordered[start:end]
        ]}
        if end < len(ordered):
            page["nextCursor"] = ordered[end - 1].id
        return page

    def llms_txt(self) -> str:
        """The llms.txt index served at `{mount}/admin/docs/llms.txt` (C20)."""
        out = (
            f"# {self.server}\n\n"
            f"> Veoveo MCP server documents. Contract revision {CONTRACT_REVISION}.\n\n"
            "## Docs\n\n"
        )
        for doc in self.docs:
            out += f"- [{doc.title}]({doc.id})\n"
        return out

    def agent_manual(self) -> str | None:
        """The agent manual embedded from the package `AGENTS.md`, when present."""
        doc = self.doc(DOC_ID_AGENTS)
        return doc.body if doc is not None else None


class ComplianceStatus(str, Enum):
    """Declared status of one checklist item."""

    MET = "met"
    PENDING = "pending"


@dataclass(frozen=True)
class ComplianceItem:
    """One checklist item as declared in a server's `Contract Compliance` section."""

    id: str
    status: ComplianceStatus
    note: str | None = None

    def wire(self) -> dict[str, str]:
        item: dict[str, str] = {"id": self.id, "status": self.status.value}
        if self.note is not None:
            item["note"] = self.note
        return item


@dataclass(frozen=True)
class ContractDeclaration:
    """The machine-readable declaration served at `{scheme}://contract` (C19)."""

    server: str
    contract_revision: int
    compliance: tuple[ComplianceItem, ...]
    @classmethod
    def from_docs(cls, docs: ServerDocs) -> "ContractDeclaration":
        """Builds the declaration from the embedded agent manual so the served
        declaration and the package `AGENTS.md` cannot diverge."""
        manual = docs.agent_manual()
        compliance = tuple(parse_compliance(manual)) if manual is not None else ()
        return cls(
            server=docs.server,
            contract_revision=CONTRACT_REVISION,
            compliance=compliance,
        )

    def wire(self) -> dict[str, Any]:
        return {
            "server": self.server,
            "contract_revision": self.contract_revision,
            "compliance": [item.wire() for item in self.compliance],
        }


def parse_compliance(manual: str) -> list[ComplianceItem]:
    """Parses `- Cnn: met` and `- Cnn: pending — reason` lines from the
    `## Contract Compliance` section of an agent manual."""
    in_section = False
    items: list[ComplianceItem] = []
    for line in manual.splitlines():
        trimmed = line.strip()
        if trimmed.startswith("## "):
            in_section = trimmed == "## Contract Compliance"
            continue
        if not in_section:
            continue
        if not trimmed.startswith("- C"):
            continue
        entry = trimmed[len("- C") :]
        number, separator, rest = entry.partition(":")
        if not separator:
            continue
        item_id = f"C{number.strip()}"
        rest = rest.strip()
        if rest.startswith("met"):
            status, remainder = ComplianceStatus.MET, rest[len("met") :]
        elif rest.startswith("pending"):
            status, remainder = ComplianceStatus.PENDING, rest[len("pending") :]
        else:
            continue
        note = remainder.lstrip(" —-").strip()
        items.append(
            ComplianceItem(id=item_id, status=status, note=note if note else None)
        )
    return items


def server_docs(
    server: str, package: str, source_root: Path | None = None
) -> ServerDocs:
    """Loads the package's embedded `AGENTS.md` and `DESIGN.md` as its served
    document set (C18, C21) — the Python analog of the Rust `server_docs!`
    macro.

    Each document is read from the installed package data first
    (`<package>/AGENTS.md`, placed there by the wheel build) and from
    `source_root` when running from a source tree. Missing and empty
    documents raise :class:`ServerDocsError`, so a server that would serve an
    incomplete manual fails at import instead."""
    package_root = files(package)
    manifest_file = package_root.joinpath("_documents.json")
    manifest = json.loads(manifest_file.read_text(encoding="utf-8")) if manifest_file.is_file() else None
    if manifest is not None:
        if not isinstance(manifest, dict) or set(manifest) != {DOC_ID_AGENTS, DOC_ID_DESIGN}:
            raise ServerDocsError("invalid packaged document manifest")
        manifest = {doc_id: ContentDigest.model_validate(value) for doc_id, value in manifest.items()}
    return ServerDocs(
        server=server,
        docs=(
            _load_doc(DOC_ID_AGENTS, DOC_TITLE_AGENTS, package, "AGENTS.md", source_root, manifest),
            _load_doc(DOC_ID_DESIGN, DOC_TITLE_DESIGN, package, "DESIGN.md", source_root, manifest),
        ),
    )


def _load_doc(
    doc_id: str,
    title: str,
    package: str,
    filename: str,
    source_root: Path | None,
    manifest: dict[str, ContentDigest] | None,
) -> ServerDoc:
    candidates: list[Any] = [files(package).joinpath(filename)]
    if source_root is not None:
        candidates.append(source_root / filename)
    for candidate in candidates:
        if candidate.is_file():
            packaged = candidate == candidates[0]
            if packaged and manifest is None:
                raise ServerDocsError("packaged documents require build digests; enable the document build hook")
            body = candidate.read_bytes().decode("utf-8")
            if not body.strip():
                raise ServerDocsError(
                    f"server document `{doc_id}` at `{candidate}` is empty"
                )
            return ServerDoc(id=doc_id, title=title, body=body,
                digest=manifest[doc_id] if packaged and manifest else None)
    searched = ", ".join(str(candidate) for candidate in candidates)
    raise ServerDocsError(
        f"server document `{doc_id}` ({filename}) not found; searched: {searched}"
    )
