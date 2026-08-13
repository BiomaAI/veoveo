"""The datasheet well-known surface: embedded documents and the declaration.

Loaded at import through `veoveo_mcp.contract.docs`, so a build whose
`AGENTS.md` or `DESIGN.md` is missing or empty fails before it can serve an
incomplete manual (contract C18-C21). Installed wheels read the documents
embedded beside the package; source-tree runs read this directory's parents.
"""

from __future__ import annotations

from pathlib import Path

from veoveo_mcp.contract import (
    ContractDeclaration,
    ServerDocs,
    server_docs,
)

from . import uris

_PACKAGE = __package__ or "datasheet_mcp"
_SOURCE_ROOT = Path(__file__).resolve().parents[2]

SERVER_DOCS: ServerDocs = server_docs(
    uris.SCHEME, _PACKAGE, source_root=_SOURCE_ROOT
)

LLMS_TXT: str = SERVER_DOCS.llms_txt()

CONTRACT_DECLARATION = ContractDeclaration.from_docs(SERVER_DOCS)
