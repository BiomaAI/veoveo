"""Admission and independent use of foundational owner types."""
import subprocess
import sys

import pytest
from pydantic import BaseModel, TypeAdapter, ValidationError

from veoveo_mcp.types import (
    ResourceScheme, ResourceTemplateUri, ResourceUri, ResourceUriBuilder,
    ScopeEnum, ScopeName, UriAuthority, UriSegment,
)
from veoveo_mcp.contract.artifacts import ArtifactId


def test_foundational_import_does_not_load_protocol_or_runtime():
    result = subprocess.run([sys.executable, '-c', '''
import sys
from veoveo_mcp.types import ResourceUri, ScopeEnum
assert not any(name == "mcp" or name.startswith(("mcp.", "surrealdb", "veoveo_mcp.contract", "veoveo_mcp.tasks")) for name in sys.modules)
class IndependentScope(ScopeEnum):
    READ = "independent:read"
assert IndependentScope.READ.scope_name() == "independent:read"
assert ResourceUri("independent://items")
'''], capture_output=True, text=True, timeout=10)
    assert result.returncode == 0, result.stderr


def test_scope_identity_survives_wire_decoding():
    class OwnerScope(ScopeEnum):
        READ = "owner:read"
    class Claims(BaseModel):
        scopes: set[ScopeName]
    value = Claims.model_validate_json('{"scopes":["owner:read"]}')
    assert all(type(scope) is ScopeName for scope in value.scopes)
    assert OwnerScope.READ.scope_name() in value.scopes
    assert value.model_dump(mode="json") == {"scopes": ["owner:read"]}
    # External names follow the shared Veoveo identifier profile.
    assert ScopeName("équipe:read")
    with pytest.raises(ValueError, match="OAuth"):
        class BadScope(ScopeEnum):
            READ = "owner:read other:write"
    with pytest.raises(ValueError, match="duplicate"):
        class AliasedScope(ScopeEnum):
            READ = "owner:read"
            ALSO_READ = "owner:read"


@pytest.mark.parametrize("value", ["", " scope", "scope ", "a\nb", "a\x7fb", "a" * 257])
def test_scope_rejects_invalid_external_names(value):
    with pytest.raises(ValueError):
        ScopeName(value)
    with pytest.raises(ValidationError):
        TypeAdapter(ScopeName).validate_python(value)


@pytest.mark.parametrize("value", ["items/one", "example://", "example://items/%xx", "example://items/%", "example://items/{id}", "example://items/a b", "example://items/é", "EXAMPLE://items", "example://items/\n"])
def test_concrete_uri_rejects_templates_and_repaired_text(value):
    with pytest.raises(ValueError):
        ResourceUri(value)


@pytest.mark.parametrize("value", ["example://user@items/path", "example://items:80/path", "example://items/path#fragment", "example://items/%FF", "example://items/%00", "example://items/..", "example://items/", "example://items?cursor=x&cursor=y", "example://items?=value"])
def test_domain_components_reject_ambiguous_or_disallowed_addresses(value):
    with pytest.raises(ValueError):
        ResourceUri(value).components()


def test_resource_builder_encodes_reserved_segments_and_query_values():
    segment = UriSegment("a/b ?é%#")
    uri = (ResourceUriBuilder(ResourceScheme("owner"), UriAuthority("items"))
           .segment(segment).query_pair("cursor", "a+b &/?é").build())
    assert type(uri) is ResourceUri
    parts = uri.components()
    assert parts.segments == (segment,)
    assert parts.query == (("cursor", "a+b &/?é"),)
    assert "%2F" in uri and "%C3%A9" in uri
    assert ResourceUri("https://example.com:443/a#section")  # generic network references
    with pytest.raises(TypeError):
        ResourceUriBuilder("owner", "items")
    with pytest.raises(TypeError):
        ResourceUriBuilder(ResourceScheme("owner"), UriAuthority("items")).segment("raw")
    with pytest.raises(ValueError):
        ResourceUriBuilder(ResourceScheme("owner"), UriAuthority("items"), query=(("cursor", "x"), ("cursor", "y")))


@pytest.mark.parametrize("value", ["example://items/{id", "example://items/{=id}", "example://items/{id=default}", "example://items/{id[]}", "{scheme}://items/{id}", "example://items/{id:0}", "items/{id}"])
def test_template_rejects_invalid_or_nonstandard_declarations(value):
    with pytest.raises(ValueError):
        ResourceTemplateUri(value)


def test_template_expansion_returns_a_concrete_reference():
    template = ResourceTemplateUri("owner://items/{id}{?cursor}")
    uri = template.expand(id="a/b ?", cursor="a+b")
    assert uri.components().segments == ("a/b ?",)
    assert uri.components().query == (("cursor", "a+b"),)
    with pytest.raises(ValueError):
        template.expand(wrong="x")
    with pytest.raises(ValueError):
        ResourceUri(template)


def test_artifact_id_stays_nominal_after_pydantic_validation():
    value = "019ffdb2-0598-7476-96d3-f3d7b0769f9e"
    parsed = TypeAdapter(ArtifactId).validate_python(value)
    assert type(parsed) is ArtifactId
    assert TypeAdapter(ArtifactId).dump_json(parsed) == ('"' + value + '"').encode()
    for invalid in [value.upper(), "not-a-uuid", "550e8400-e29b-41d4-a716-446655440000"]:
        with pytest.raises(ValueError):
            ArtifactId(invalid)
