"""Well-known surface contracts, mirroring `veoveo_mcp_contract::docs` tests."""

import json
from pathlib import Path

import pytest

from veoveo_mcp.contract import (
    ComplianceProfile,
    RequirementId,
    requirement_catalog,
    CONTRACT_REVISION,
    ComplianceStatus,
    ContractDeclaration,
    DOC_ID_AGENTS,
    DOC_ID_DESIGN,
    DOC_TITLE_AGENTS,
    DOC_TITLE_DESIGN,
    ServerDoc,
    ServerDocs,
    ServerDocsError,
    server_docs,
)

SHARED = Path(__file__).resolve().parents[3] / "mcp/contract/testdata"
PROFILE = ComplianceProfile((SHARED / "compliance-example.json").read_bytes(), requirement_catalog())
MANUAL = (SHARED / "compliance-example.md").read_text(encoding="utf-8")


def _docs(*docs: ServerDoc) -> ServerDocs:
    return ServerDocs(server="example", docs=docs, profile=PROFILE)


def test_admitted_profile_agrees_with_shared_rendered_manual():
    PROFILE.check_manual(MANUAL)
    assert [item.id for item in PROFILE.compliance] == [item.value for item in RequirementId]


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
        "> Veoveo MCP server documents. Contract revision 4.\n\n"
        "## Docs\n\n"
        "- [Agent work manual](agents)\n"
        "- [Domain design](design)\n"
    )


def test_declaration_requires_checked_profile_and_exact_embedded_manual():
    docs = _docs(ServerDoc(id=DOC_ID_AGENTS, title=DOC_TITLE_AGENTS, body=MANUAL))
    declaration = ContractDeclaration.from_docs(docs)
    assert declaration.wire() == PROFILE.wire()
    assert declaration.catalog_revision == 2
    with pytest.raises(ServerDocsError):
        ContractDeclaration.from_docs(ServerDocs("example", docs.docs))
    with pytest.raises(ServerDocsError):
        ContractDeclaration.from_docs(_docs(ServerDoc("agents", "Manual", MANUAL.replace("Contract revision: 4", "Contract revision: 3"))))


def test_requirement_enum_is_derived_from_generated_catalog():
    assert tuple(item.value for item in RequirementId) == requirement_catalog().ids


def test_docs_index_wire_never_carries_bodies():
    docs = _docs(ServerDoc(id=DOC_ID_AGENTS, title=DOC_TITLE_AGENTS, body=MANUAL))
    assert docs.index_wire() == {"items": [{"id": "agents", "title": "Agent work manual", "uri": "example://docs/agents"}]}


def test_server_docs_loads_from_a_source_root(tmp_path: Path):
    (tmp_path / "AGENTS.md").write_text(MANUAL, encoding="utf-8")
    (tmp_path / "DESIGN.md").write_text("# Design\n", encoding="utf-8")
    (tmp_path / "contract-compliance.json").write_text(json.dumps(PROFILE.wire()), encoding="utf-8")
    docs = server_docs("example", "veoveo_mcp.contract", source_root=tmp_path)
    assert docs.agent_manual() == MANUAL
    design = docs.doc(DOC_ID_DESIGN)
    assert design is not None
    assert design.body == "# Design\n"


def test_server_docs_fails_closed_on_a_missing_document(tmp_path: Path):
    (tmp_path / "AGENTS.md").write_text(MANUAL, encoding="utf-8")
    with pytest.raises(ServerDocsError):
        server_docs("example", "veoveo_mcp.contract", source_root=tmp_path)


def test_server_docs_fails_closed_on_an_empty_document(tmp_path: Path):
    (tmp_path / "AGENTS.md").write_text("  \n", encoding="utf-8")
    (tmp_path / "DESIGN.md").write_text("# Design\n", encoding="utf-8")
    with pytest.raises(ServerDocsError):
        server_docs("example", "veoveo_mcp.contract", source_root=tmp_path)


def test_blank_document_bodies_are_rejected_at_construction():
    with pytest.raises(ServerDocsError):
        ServerDoc(id=DOC_ID_AGENTS, title=DOC_TITLE_AGENTS, body=" ")


def test_shared_valid_and_invalid_profile_bytes():
    fixtures = json.loads((SHARED / "compliance-profiles.json").read_bytes())
    for case in fixtures["valid"]:
        profile = ComplianceProfile(json.dumps(case["profile"]), requirement_catalog())
        assert len(profile.compliance) == len(requirement_catalog().ids), case["name"]
    for case in fixtures["invalid"]:
        with pytest.raises(ValueError):
            ComplianceProfile(json.dumps(case["profile"]), requirement_catalog())


def test_profile_refuses_public_mutation_and_duplicate_json_fields():
    with pytest.raises(AttributeError):
        PROFILE.server = "other"
    with pytest.raises(ValueError):
        ComplianceProfile('{"server":"a","server":"b"}', requirement_catalog())


def test_manual_markers_and_body_fail_closed():
    for corrupt in [MANUAL.replace("<!-- veoveo:contract-compliance:start -->", ""),
                    MANUAL + "<!-- veoveo:contract-compliance:end -->",
                    MANUAL.replace("Catalog revision: 2", "Catalog revision: 1"),
                    MANUAL.replace("## Contract Compliance", "## Build And Test")]:
        with pytest.raises(ValueError):
            PROFILE.check_manual(corrupt)
    PROFILE.check_manual("Manual extension\n" + MANUAL + "\nExtra manual section\n")


def test_catalog_conditions_require_actual_discovery_agreement():
    cases = json.loads((SHARED / "compliance-profiles.json").read_bytes())["valid"]
    for case in cases:
        profile = ComplianceProfile(json.dumps(case["profile"]), requirement_catalog())
        conditional = [item for item in profile.compliance if item.status is ComplianceStatus.NOT_APPLICABLE]
        if conditional:
            profile.check_applicability(knowledge_source=False)
            with pytest.raises(ValueError):
                profile.check_applicability(knowledge_source=True)
        else:
            profile.check_applicability(knowledge_source=True)
            with pytest.raises(ValueError):
                profile.check_applicability(knowledge_source=False)


def test_catalog_growth_refuses_an_old_partial_profile():
    catalog = json.loads((Path(__file__).resolve().parents[1] / "src/veoveo_mcp/catalog/requirements.json").read_bytes())
    catalog["requirements"].append({"id": "C34", "level": "must", "text": "Simulated additional requirement"})
    from veoveo_mcp._compliance import RequirementCatalog
    with pytest.raises(ValueError, match="generated revision"):
        ComplianceProfile(PROFILE.wire(), RequirementCatalog(json.dumps(catalog)))


def test_profile_has_no_subclass_admission_bypass():
    with pytest.raises(TypeError):
        type("UncheckedProfile", (ComplianceProfile,), {})


def test_prior_declaration_field_spellings_are_not_aliases():
    for current, obsolete in [("contractRevision", "contract_revision"), ("catalogRevision", "catalog_revision")]:
        value = PROFILE.wire()
        value[obsolete] = value.pop(current)
        with pytest.raises(ValueError):
            ComplianceProfile(json.dumps(value), requirement_catalog())
