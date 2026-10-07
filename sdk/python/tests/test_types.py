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


def test_chrono_timestamp_precision_calendar_and_explicit_conversion():
    from datetime import datetime, timezone
    from dataclasses import FrozenInstanceError
    from veoveo_mcp.types import ChronoTimestamp

    first = ChronoTimestamp("2026-10-05T12:34:56.123456001Z")
    second = ChronoTimestamp("2026-10-05T12:34:56.123456002Z")
    assert not first.same_instant(second)
    assert first.same_instant(ChronoTimestamp("2026-10-05T14:34:56.123456001+02:00"))
    assert first.instant_parts()[1] == 123456001
    with pytest.raises(TypeError):
        first < second
    with pytest.raises(FrozenInstanceError):
        first.wire = "invalid"
    with pytest.raises(ValueError, match="exactly"):
        first.as_datetime_exact()
    assert first.as_datetime_lossy_microseconds().microsecond == 123456
    exact = datetime(2026, 10, 5, 12, 34, 56, 123456, tzinfo=timezone.utc)
    assert ChronoTimestamp.from_datetime(exact).as_datetime_exact() == exact
    with pytest.raises(ValueError, match="aware"):
        ChronoTimestamp.from_datetime(exact.replace(tzinfo=None))
    leap = ChronoTimestamp("2016-12-31T23:59:60.123456789Z")
    assert leap.instant_parts()[1] == 1_123_456_789
    for conversion in [leap.as_datetime_exact, leap.as_datetime_lossy_microseconds]:
        with pytest.raises(ValueError, match="leap"):
            conversion()
    for text in ["0000-02-29T00:00:00Z", "-0400-02-29T00:00:00Z",
                 "+10000-02-29T00:00:00Z", "-262143-01-01T00:00:00Z",
                 "+262142-12-31T23:59:59.999999999Z"]:
        value = ChronoTimestamp(text)
        assert str(value) == text
        with pytest.raises(ValueError):
            value.as_datetime_exact()



@pytest.mark.parametrize("wire", [
    None, True, 1, 1.5, "", "2026-1-2 3:4:5 UTC", "2026-01-01T00:00:00+0000",
    "2026-01-01t00:00:00z", "2026-01-01T00:00:00.123456789123Z", "2026-01-01T00:00:00", "2026-01-01T00:00:00+24:00",
    "2026-01-01T00:00:00+00:60", "2026-02-29T00:00:00Z", "-0100-02-29T00:00:00Z",
    "2026-01-01T24:00:00Z", "2026-01-01T00:60:00Z", "2026-01-01T00:00:61Z",
    "2026-01-01T00:00:00.Z", "+262143-01-01T00:00:00Z", "-262144-12-31T23:59:59Z",
    "-262143-01-01T00:00:00+00:01", "+262142-12-31T23:59:59-00:01",
])
def test_chrono_timestamp_all_decoder_paths_reject_invalid_values(wire):
    import json
    from veoveo_mcp.types import ChronoTimestamp
    adapter = TypeAdapter(ChronoTimestamp)
    for decode in [lambda: adapter.validate_python(wire),
                   lambda: adapter.validate_json(json.dumps(wire))]:
        with pytest.raises(ValueError):
            decode()


def test_chrono_timestamp_schema_and_forged_nominal_instance():
    from veoveo_mcp.types import ChronoTimestamp
    adapter = TypeAdapter(ChronoTimestamp)
    schema = adapter.json_schema()
    assert schema["type"] == "string" and "pattern" in schema
    assert "format" not in schema
    forged = object.__new__(ChronoTimestamp)
    object.__setattr__(forged, "wire", "2026-02-30T00:00:00Z")
    with pytest.raises(ValueError):
        adapter.validate_python(forged)
