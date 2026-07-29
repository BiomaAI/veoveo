"""The datasheet well-known surface: embedded documents and the declaration.

Loaded at import through `veoveo_mcp.contract.docs`, so a build whose
`AGENTS.md` or `DESIGN.md` is missing or empty fails before it can serve an
incomplete manual (contract C18-C21). Installed wheels read the documents
embedded beside the package; source-tree runs read this directory's parents.
"""

from __future__ import annotations

from pathlib import Path

from veoveo_mcp.contract import (
    CapabilityInventory,
    ContractDeclaration,
    DOC_ID_AGENTS,
    DOC_ID_DESIGN,
    ServerDocs,
    server_docs,
)

from . import prompts, uris

_PACKAGE = __package__ or "datasheet_mcp"
_SOURCE_ROOT = Path(__file__).resolve().parents[2]

SERVER_DOCS: ServerDocs = server_docs(
    uris.SCHEME, _PACKAGE, source_root=_SOURCE_ROOT
)

LLMS_TXT: str = SERVER_DOCS.llms_txt()

CAPABILITY_INVENTORY = CapabilityInventory(
    tools=("column_stats", "preview_dataset", "profile_dataset"),
    resources=(
        uris.REPORTS_URI,
        uris.USAGE_ROOT_URI,
        uris.DOCS_URI,
        uris.doc_uri(DOC_ID_AGENTS),
        uris.doc_uri(DOC_ID_DESIGN),
        uris.CONTRACT_URI,
    ),
    resource_templates=(uris.USAGE_TASK_TEMPLATE, uris.ARTIFACT_TEMPLATE),
    prompts=(prompts.PROFILE_PROMPT, prompts.REVIEW_PROMPT),
    tasks=("profile_dataset",),
)

CONTRACT_DECLARATION = ContractDeclaration.from_docs(
    SERVER_DOCS, CAPABILITY_INVENTORY
)
