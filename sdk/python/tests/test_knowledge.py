from hashlib import sha256
import json

import mcp.types as types
import pytest
from pydantic import ValidationError
from veoveo_mcp.contract.docs import ServerDoc, ServerDocs, ServerDocsError
from veoveo_mcp.contract.knowledge import (
    EXTENSION_ID, OBSERVATION_KEY, AccessDescriptor, CollectionDescriptor, ContentDigest, ImmutableFreshness,
    Observation, Revision, member_result,
)


def documents():
    return ServerDocs(server="fixture", docs=(ServerDoc(id="design", title="Design", body="# Design\r\nExact bytes.\n"),))


def test_read_policy_is_explicit_and_closed():
    fields = dict(tenant="tenant", workContext="context", owner={"kind": "principal", "id": "author"}, dataLabels=[])
    for policy in ({"kind": "tenant"}, {"kind": "subjects"}, {"kind": "work-context"},
                   {"kind": "selected-work-context"}, {"kind": "subjects-in-context"}, {"kind": "subjects-in-context", "profile": "operations"}):
        value = AccessDescriptor.model_validate({**fields, "readPolicy": policy})
        assert value.wire()["readPolicy"] == policy
    with pytest.raises(ValidationError):
        AccessDescriptor.model_validate(fields)
    for policy in ({"kind": "unknown"}, {"kind": "subjects", "profile": "operations"},
                   {"kind": "subjects-in-context", "caller": "author"},
                   {"kind": "subjects-in-context", "profile": "Operations"},
                   {"kind": "subjects-in-context", "profile": "operations/read"}):
        with pytest.raises(ValidationError):
            AccessDescriptor.model_validate({**fields, "readPolicy": policy})


def test_closed_collection_and_observation_models_reject_invalid_relationships():
    collection = documents().collection()
    wire = collection.wire()
    assert CollectionDescriptor.model_validate(wire) == collection
    for changes in (
        {"freshness": {"immutable": False}},
        {"freshness": {"immutable": True, "maxAgeSeconds": 1}},
        {"changeSignal": "listen"},
        {"callerPayload": "extra"},
        {"collection": "unqualified"},
    ):
        with pytest.raises(ValidationError):
            CollectionDescriptor.model_validate({**wire, **changes})
    with pytest.raises(ValidationError):
        ImmutableFreshness(immutable=1)
    with pytest.raises(ValidationError):
        Revision("line\nbreak")


def test_authorized_docs_negotiate_conditions_and_bind_exact_bytes():
    docs = documents()
    uri = "fixture://docs/design"
    ordinary = docs.read_authorized(uri)
    assert ordinary.meta is None
    capabilities = types.ClientCapabilities(extensions={EXTENSION_ID: {}})
    full = docs.read_authorized(uri, capabilities=capabilities, metadata={OBSERVATION_KEY: {"revision": "forged"}})
    observed = Observation.model_validate(full.meta[OBSERVATION_KEY])
    assert str(observed.content_sha256) == sha256(full.contents[0].text.encode()).hexdigest()
    assert observed.revision == Revision(str(observed.content_sha256))
    unchanged = docs.read_authorized(uri, capabilities=capabilities,
        metadata={EXTENSION_ID: {"ifNoneMatch": str(observed.revision)}})
    assert unchanged.contents == []
    assert Observation.model_validate(unchanged.meta[OBSERVATION_KEY]).not_modified
    assert unchanged.ttl_ms == 0
    unnegotiated = docs.read_authorized(uri, metadata={EXTENSION_ID: {"ifNoneMatch": str(observed.revision)}})
    assert unnegotiated.contents
    with pytest.raises(ValueError):
        member_result(uri=uri, text="changed", mime_type="text/plain", observation=observed,
            collection=docs.collection(), capabilities=capabilities)
    with pytest.raises(ValueError):
        docs.read_authorized(uri, capabilities=types.ClientCapabilities(extensions={EXTENSION_ID: {"unknown": True}}))


def test_docs_paging_and_uri_admission_are_shared():
    docs = ServerDocs(server="fixture", docs=tuple(ServerDoc(id=f"doc-{i:03}", title="Doc", body="body") for i in range(35)))
    first = json.loads(docs.read_authorized("fixture://docs").contents[0].text)
    assert len(first["items"]) == 32
    second = json.loads(docs.read_authorized("fixture://docs?cursor=" + first["nextCursor"]).contents[0].text)
    assert len(second["items"]) == 3 and "nextCursor" not in second
    assert docs.read_authorized("other://docs") is None
    for uri in ("fixture://docs?cursor=a&cursor=b", "fixture://docs?unknown=a", "fixture://docs/doc-001?cursor=a", "fixture://docs/doc-001#fragment"):
        with pytest.raises(ValueError):
            docs.read_authorized(uri)
    with pytest.raises(ServerDocsError, match="digest mismatch"):
        ServerDoc(id="design", title="Design", body="body", digest=ContentDigest.of("other"))


def test_packaged_docs_require_build_digests_and_preserve_bytes(tmp_path, monkeypatch):
    from veoveo_mcp.contract import docs as module
    monkeypatch.setattr(module, "files", lambda package: tmp_path)
    body = b"# Documentation\r\nExact packaged bytes.\n"
    for name in ("AGENTS.md", "DESIGN.md"):
        (tmp_path / name).write_bytes(body)
    with pytest.raises(ServerDocsError, match="require build digests"):
        module.server_docs("fixture", "fixture")
    digest = sha256(body).hexdigest()
    (tmp_path / "_documents.json").write_text(json.dumps({"agents": digest, "design": digest}))
    docs = module.server_docs("fixture", "fixture")
    assert docs.doc("design").body.encode() == body
    assert str(docs.doc("design").digest) == digest
    (tmp_path / "DESIGN.md").write_bytes(body + b"tampered")
    with pytest.raises(ServerDocsError, match="digest mismatch"):
        module.server_docs("fixture", "fixture", source_root=tmp_path)


def test_access_deadlines_and_typed_grants_are_closed_and_timezone_aware():
    fields = dict(tenant="tenant", workContext="context", owner={"kind": "principal", "id": "author"},
                  readPolicy={"kind": "selected-work-context"}, dataLabels=[], expiresAt="2026-10-01T00:00:00Z",
                  grants=[{"subject": {"kind": "group", "id": "readers"}, "expiresAt": "2026-10-01T00:00:00Z"}])
    assert AccessDescriptor.model_validate(fields).wire() == fields
    for grants in ([{"kind": "principal", "id": "reader"}],
                   [{"subject": {"kind": "principal", "id": "reader"}, "expiresAt": "2026-10-01T00:00:00"}],
                   [{"subject": {"kind": "principal", "id": "reader"}, "deadline": "2026-10-01T00:00:00Z"}]):
        with pytest.raises(ValidationError):
            AccessDescriptor.model_validate({**fields, "grants": grants})
    with pytest.raises(ValidationError):
        AccessDescriptor.model_validate({**fields, "expiresAt": "2026-10-01T00:00:00"})
