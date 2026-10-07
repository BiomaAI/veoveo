"""A new owner qualifies checked setup without registration in MCP core."""
from dataclasses import FrozenInstanceError
from enum import Enum

import mcp.types as types
import pytest
from mcp.server import Server

from pathlib import Path
from veoveo_mcp.contract.docs import ComplianceProfile, ServerDoc, ServerDocs, requirement_catalog
from veoveo_mcp.contract.knowledge import EXTENSION_ID
from veoveo_mcp.contract.server import McpResource, McpResourceTemplate, McpServerSetup
from veoveo_mcp.types import ResourceScheme, ResourceTemplateUri, ResourceUri, ScopeEnum


class FixtureScope(ScopeEnum):
    READ = "independent:read"
    WRITE = "independent:write"


class FixtureResource(Enum):
    DOCS = ResourceUri("independent://docs")
    CONTRACT = ResourceUri("independent://contract")
    AGENTS = ResourceUri("independent://docs/agents")
    DESIGN = ResourceUri("independent://docs/design")

    def to_uri(self) -> ResourceUri:
        return self.value


class IndependentContract:
    name = "independent"
    version = "1.0.0"
    instructions = "Independent server"
    scheme = ResourceScheme("independent")
    scope_type = FixtureScope
    scopes = (FixtureScope.READ,)
    _profile_bytes = (Path(__file__).resolve().parents[3] / "mcp/contract/testdata/compliance-example.json").read_text().replace('"example"', '"independent"')
    _profile = ComplianceProfile(_profile_bytes, requirement_catalog())
    documents = ServerDocs(name, (
        ServerDoc("agents", "Agents", "## Contract Compliance\n\n<!-- veoveo:contract-compliance:start -->\n" + _profile.render() + "<!-- veoveo:contract-compliance:end -->\n"),
        ServerDoc("design", "Design", "# Design"),
    ), _profile)

    def __init__(self):
        self.entries = tuple(McpResource.build(address, lambda uri: types.Resource(uri=uri, name=uri))
                             for address in FixtureResource)
        template = ResourceTemplateUri("independent://docs/{doc_id}")
        self.templates = (McpResourceTemplate.build(template, lambda uri: types.ResourceTemplate(uri_template=uri, name="docs")),)

    def parse_resource(self, uri):
        return FixtureResource(uri)

    def resources(self):
        return self.entries

    def resource_templates(self):
        return self.templates


def test_independent_owner_setup_attaches_docs_and_copies_descriptors():
    contract = IndependentContract()
    setup = McpServerSetup(contract)
    assert setup.scope_names == {FixtureScope.READ.scope_name()}
    assert setup.resource_templates()[0].meta[EXTENSION_ID]["collection"] == "independent.docs"
    contract.entries[0].descriptor.uri = "independent://mutated"
    copies = setup.resources()
    copies[0].uri = "independent://changed"
    assert {resource.uri for resource in setup.resources()} == {resource.value for resource in FixtureResource}
    with pytest.raises(FrozenInstanceError):
        setup.name = "changed"


def test_permission_membership_requires_the_owner_type_and_declared_scope():
    class ForeignScope(ScopeEnum):
        READ = "independent:read"
    setup = McpServerSetup(IndependentContract())
    grants = {FixtureScope.READ.scope_name(), FixtureScope.WRITE.scope_name()}
    assert setup.has_scope(grants, FixtureScope.READ)
    assert not setup.has_scope(set(), FixtureScope.READ)
    assert not setup.has_scope(grants, FixtureScope.WRITE)
    for wrong in ("independent:read", ForeignScope.READ):
        with pytest.raises(TypeError):
            setup.has_scope(grants, wrong)


@pytest.mark.parametrize("fault", ["identity", "version", "scopes", "resource_duplicate", "descriptor", "round_trip", "docs_root", "contract", "document", "missing_doc", "duplicate_doc", "template_missing", "template_duplicate", "template_descriptor"])
def test_checked_setup_rejects_broken_owner_contract(fault):
    owner = IndependentContract()
    match fault:
        case "identity": owner.name = "wrong"
        case "version": owner.version = ""
        case "scopes": owner.scopes = (FixtureScope.READ, FixtureScope.READ)
        case "resource_duplicate": owner.entries += owner.entries[:1]
        case "descriptor": owner.entries[0].descriptor.uri = "independent://wrong"
        case "round_trip": owner.parse_resource = lambda _uri: FixtureResource.DOCS
        case "docs_root": owner.entries = tuple(e for e in owner.entries if e.address != FixtureResource.DOCS)
        case "contract": owner.entries = tuple(e for e in owner.entries if e.address != FixtureResource.CONTRACT)
        case "document": owner.entries = tuple(e for e in owner.entries if e.address != FixtureResource.DESIGN)
        case "missing_doc": owner.documents = ServerDocs(owner.name, owner.documents.docs[:1], owner.documents.profile)
        case "duplicate_doc": owner.documents = ServerDocs(owner.name, owner.documents.docs + owner.documents.docs[:1], owner.documents.profile)
        case "template_missing": owner.templates = ()
        case "template_duplicate": owner.templates += owner.templates
        case "template_descriptor": owner.templates[0].descriptor.uri_template = "independent://other/{doc_id}"
    with pytest.raises(ValueError):
        McpServerSetup(owner)


def test_descriptors_cannot_replace_a_checked_address():
    with pytest.raises(ValueError):
        McpResource.build(FixtureResource.DOCS, lambda _uri: types.Resource(uri="independent://other", name="other"))
    with pytest.raises(TypeError):
        McpResourceTemplate.build(ResourceUri("independent://docs"), lambda uri: types.ResourceTemplate(uri_template=uri, name="docs"))
    with pytest.raises(ValueError):
        McpResourceTemplate.build(ResourceTemplateUri("independent://docs/{doc_id}"), lambda _uri: types.ResourceTemplate(uri_template="independent://wrong/{id}", name="docs"))


def test_setup_requires_the_resources_surface_and_matching_server_identity():
    setup = McpServerSetup(IndependentContract())
    async def handler(_ctx, _params):
        raise AssertionError("construction must not dispatch a handler")
    with pytest.raises(ValueError, match="handlers"):
        setup.configure(Server(setup.name, version=setup.version))
    with pytest.raises(ValueError, match="identity"):
        setup.configure(Server("wrong", version=setup.version))
    server = Server(setup.name, version=setup.version, on_list_resources=handler,
                    on_list_resource_templates=handler, on_read_resource=handler)
    setup.configure(server)
    assert server.extensions[EXTENSION_ID] == {}
