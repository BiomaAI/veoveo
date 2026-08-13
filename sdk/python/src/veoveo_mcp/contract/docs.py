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
from typing import Any, Iterator

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
)
"""Stable identifiers of the compliance checklist in `DESIGN.md`."""


class ServerDocsError(ValueError):
    """A server document set could not be assembled fail-closed."""


@dataclass(frozen=True)
class ServerDoc:
    """One document embedded from the server package."""

    id: str
    title: str
    body: str

    def __post_init__(self) -> None:
        if not self.id.strip():
            raise ServerDocsError("server document id must be non-empty")
        if not self.title.strip():
            raise ServerDocsError(f"server document `{self.id}` title must be non-empty")
        if not self.body.strip():
            raise ServerDocsError(f"server document `{self.id}` body must be non-empty")

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

    def index_wire(self) -> list[dict[str, str]]:
        """The JSON index served at `{scheme}://docs`."""
        return [doc.wire() for doc in self.docs]

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
    return ServerDocs(
        server=server,
        docs=(
            _load_doc(DOC_ID_AGENTS, DOC_TITLE_AGENTS, package, "AGENTS.md", source_root),
            _load_doc(DOC_ID_DESIGN, DOC_TITLE_DESIGN, package, "DESIGN.md", source_root),
        ),
    )


def _load_doc(
    doc_id: str,
    title: str,
    package: str,
    filename: str,
    source_root: Path | None,
) -> ServerDoc:
    candidates: list[Any] = [files(package).joinpath(filename)]
    if source_root is not None:
        candidates.append(source_root / filename)
    for candidate in candidates:
        if candidate.is_file():
            body = candidate.read_text(encoding="utf-8")
            if not body.strip():
                raise ServerDocsError(
                    f"server document `{doc_id}` at `{candidate}` is empty"
                )
            return ServerDoc(id=doc_id, title=title, body=body)
    searched = ", ".join(str(candidate) for candidate in candidates)
    raise ServerDocsError(
        f"server document `{doc_id}` ({filename}) not found; searched: {searched}"
    )
