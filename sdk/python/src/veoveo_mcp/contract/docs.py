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

from veoveo_mcp._compliance import (
    CONTRACT_REVISION, CATALOG_REVISION, ComplianceItem, ComplianceProfile, ComplianceStatus,
    ProfileError, RequirementCatalog, decode_json,
)

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

def requirement_catalog() -> RequirementCatalog:
    return RequirementCatalog(files("veoveo_mcp").joinpath("catalog/requirements.json").read_bytes())


RequirementId = Enum("RequirementId", {item: item for item in requirement_catalog().ids}, type=str)


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
    profile: ComplianceProfile | None = None

    def __post_init__(self) -> None:
        if not self.server.strip():
            raise ServerDocsError("server name must be non-empty")
        if self.profile is not None and (not isinstance(self.profile, ComplianceProfile) or self.profile.server != self.server):
            raise ServerDocsError("document profile/server mismatch")

    def doc(self, doc_id: str) -> ServerDoc | None:
        for doc in self.docs:
            if doc.id == doc_id:
                return doc
        return None

    def __iter__(self) -> Iterator[ServerDoc]:
        return iter(self.docs)

    def collection(self, scheme: str | None = None) -> CollectionDescriptor:
        return CollectionDescriptor(
            collection=CollectionId(f"{self.server}.docs"), entityKind=EntityKind("document"),
            enumerate=ResourceUriBuilder(ResourceScheme(scheme or self.server), UriAuthority("docs")).build(),
            freshness=ImmutableFreshness(immutable=True), changeSignal=ChangeSignal.IMMUTABLE,
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


class ContractDeclaration:
    """An admitted complete profile, verified against the exact served manual."""
    __slots__ = ("_profile",)

    def __init__(self, profile: ComplianceProfile):
        if not isinstance(profile, ComplianceProfile):
            raise ServerDocsError("contract declaration requires an admitted profile")
        object.__setattr__(self, "_profile", profile)

    def __setattr__(self, name, value):
        raise AttributeError("contract declarations are immutable")

    @classmethod
    def from_docs(cls, docs: ServerDocs) -> "ContractDeclaration":
        if docs.profile is None:
            raise ServerDocsError("server documents require a complete compliance profile")
        manual = docs.agent_manual()
        if manual is None or docs.profile.server != docs.server:
            raise ServerDocsError("compliance profile/server mismatch or missing manual")
        try:
            docs.profile.check_manual(manual)
            docs.profile.check_applicability(knowledge_source=True)
        except ProfileError as error:
            raise ServerDocsError(str(error)) from error
        return cls(docs.profile)

    @property
    def server(self):
        return self._profile.server

    @property
    def contract_revision(self):
        return self._profile.contract_revision

    @property
    def catalog_revision(self):
        return self._profile.catalog_revision

    @property
    def compliance(self):
        return self._profile.compliance

    def wire(self) -> dict[str, Any]:
        return self._profile.wire()


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
    packaged = manifest_file.is_file()
    root = package_root if packaged else source_root
    if root is None:
        raise ServerDocsError("packaged documents require build digests or an explicit source root")
    try:
        manifest = decode_json(manifest_file.read_bytes()) if packaged else None
        artifact_names = {DOC_ID_AGENTS: "AGENTS.md", DOC_ID_DESIGN: "DESIGN.md",
                          "profile": "contract-compliance.json", "catalog": "requirements.json",
                          "schema": "compliance-profile.schema.json"}
        if packaged and (not isinstance(manifest, dict) or set(manifest) != set(artifact_names)):
            raise ServerDocsError("invalid packaged document manifest")
        from hashlib import sha256
        data = {}
        for key, filename in artifact_names.items():
            # Source author roots share the SDK's generated catalog export.
            candidate = root.joinpath(filename)
            if not packaged and key in {"catalog", "schema"}:
                candidate = files("veoveo_mcp").joinpath("catalog", filename)
            data[key] = candidate.read_bytes()
            if packaged and manifest[key] != sha256(data[key]).hexdigest():
                raise ServerDocsError("packaged artifact digest mismatch; rebuild the package")
        if not isinstance(decode_json(data["schema"]), dict):
            raise ServerDocsError("invalid generated compliance schema")
        catalog = RequirementCatalog(data["catalog"])
        profile = ComplianceProfile(data["profile"], catalog)
        if profile.server != server:
            raise ServerDocsError("compliance profile/server mismatch")
        documents = tuple(ServerDoc(id=key, title=title, body=data[key].decode("utf-8"))
                          for key, title in ((DOC_ID_AGENTS, DOC_TITLE_AGENTS), (DOC_ID_DESIGN, DOC_TITLE_DESIGN)))
        docs = ServerDocs(server, documents, profile)
        ContractDeclaration.from_docs(docs)
        return docs
    except (OSError, UnicodeError, ProfileError) as error:
        raise ServerDocsError(f"server document/profile loading failed: {error}") from error
