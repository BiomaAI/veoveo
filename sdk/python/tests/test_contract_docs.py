"""Well-known surface contracts, mirroring `veoveo_mcp_contract::docs` tests."""

import json
from pathlib import Path

import pytest

from veoveo_mcp.contract import (
    CHECKLIST_IDS,
    CONTRACT_REVISION,
    CapabilityInventory,
    ComplianceStatus,
    ContractDeclaration,
    DOC_ID_AGENTS,
    DOC_ID_DESIGN,
    DOC_TITLE_AGENTS,
    DOC_TITLE_DESIGN,
    ServerDoc,
    ServerDocs,
    ServerDocsError,
    parse_compliance,
    server_docs,
)

MANUAL = (
    "# Example\n\n## Purpose\n\nText.\n\n## Contract Compliance\n\n"
    "Contract revision: 2\n\n- C01: met\n"
    "- C02: pending — well-known surface not yet wired\n"
    "- C03: pending - unverified\n\n## Build And Test\n\n- cargo test\n"
)


def _docs(*docs: ServerDoc) -> ServerDocs:
    return ServerDocs(server="example", docs=docs)


def test_parses_met_and_pending_items_within_section_bounds():
    items = parse_compliance(MANUAL)
    assert len(items) == 3
    assert items[0].id == "C01"
    assert items[0].status == ComplianceStatus.MET
    assert items[0].note is None
    assert items[1].status == ComplianceStatus.PENDING
    assert items[1].note == "well-known surface not yet wired"
    assert items[2].note == "unverified"


def test_llms_txt_lists_every_document():
    docs = _docs(
        ServerDoc(id=DOC_ID_AGENTS, title=DOC_TITLE_AGENTS, body="body"),
        ServerDoc(id=DOC_ID_DESIGN, title=DOC_TITLE_DESIGN, body="body"),
    )
    index = docs.llms_txt()
    assert index.startswith("# example\n")
    assert "- [Agent work manual](agents)" in index
    assert "- [Domain design](design)" in index
    assert f"Contract revision {CONTRACT_REVISION}" in index


def test_llms_txt_renders_the_exact_served_format():
    docs = _docs(
        ServerDoc(id=DOC_ID_AGENTS, title=DOC_TITLE_AGENTS, body="body"),
        ServerDoc(id=DOC_ID_DESIGN, title=DOC_TITLE_DESIGN, body="body"),
    )
    assert docs.llms_txt() == (
        "# example\n\n"
        "> Veoveo MCP server documents. Contract revision 2.\n\n"
        "## Docs\n\n"
        "- [Agent work manual](agents)\n"
        "- [Domain design](design)\n"
    )


def test_declaration_derives_from_the_embedded_manual():
    docs = _docs(ServerDoc(id=DOC_ID_AGENTS, title=DOC_TITLE_AGENTS, body=MANUAL))
    declaration = ContractDeclaration.from_docs(docs, CapabilityInventory())
    assert declaration.server == "example"
    assert declaration.contract_revision == CONTRACT_REVISION
    assert len(declaration.compliance) == 3
    wire = declaration.wire()
    assert json.loads(json.dumps(wire)) == wire
    assert wire == {
        "server": "example",
        "contract_revision": 2,
        "compliance": [
            {"id": "C01", "status": "met"},
            {
                "id": "C02",
                "status": "pending",
                "note": "well-known surface not yet wired",
            },
            {"id": "C03", "status": "pending", "note": "unverified"},
        ],
        "capabilities": {},
    }


def test_checklist_ids_are_dense_and_stable():
    assert len(CHECKLIST_IDS) == 30
    for index, checklist_id in enumerate(CHECKLIST_IDS):
        assert checklist_id == f"C{index + 1:02}"


def test_capability_inventory_wire_omits_empty_lists():
    inventory = CapabilityInventory(
        tools=("preview",), tasks=("profile",)
    )
    assert inventory.wire() == {"tools": ["preview"], "tasks": ["profile"]}
    assert CapabilityInventory().wire() == {}


def test_docs_index_wire_never_carries_bodies():
    docs = _docs(ServerDoc(id=DOC_ID_AGENTS, title=DOC_TITLE_AGENTS, body=MANUAL))
    assert docs.index_wire() == [{"id": "agents", "title": "Agent work manual"}]


def test_server_docs_loads_from_a_source_root(tmp_path: Path):
    (tmp_path / "AGENTS.md").write_text(MANUAL, encoding="utf-8")
    (tmp_path / "DESIGN.md").write_text("# Design\n", encoding="utf-8")
    docs = server_docs("example", "veoveo_mcp.contract", source_root=tmp_path)
    assert docs.agent_manual() == MANUAL
    design = docs.doc(DOC_ID_DESIGN)
    assert design is not None
    assert design.body == "# Design\n"


def test_server_docs_fails_closed_on_a_missing_document(tmp_path: Path):
    (tmp_path / "AGENTS.md").write_text(MANUAL, encoding="utf-8")
    with pytest.raises(ServerDocsError, match="design"):
        server_docs("example", "veoveo_mcp.contract", source_root=tmp_path)


def test_server_docs_fails_closed_on_an_empty_document(tmp_path: Path):
    (tmp_path / "AGENTS.md").write_text("  \n", encoding="utf-8")
    (tmp_path / "DESIGN.md").write_text("# Design\n", encoding="utf-8")
    with pytest.raises(ServerDocsError, match="empty"):
        server_docs("example", "veoveo_mcp.contract", source_root=tmp_path)


def test_blank_document_bodies_are_rejected_at_construction():
    with pytest.raises(ServerDocsError):
        ServerDoc(id=DOC_ID_AGENTS, title=DOC_TITLE_AGENTS, body=" ")
