import asyncio
import base64
import hashlib
import json
from datetime import datetime, timezone

import httpx
import pytest
from uuid_utils.compat import uuid7

from veoveo_mcp.artifacts import (
    ArtifactDenied, ArtifactRepository, ArtifactTooLarge, ArtifactTransport, HttpArtifactPlane,
)
from veoveo_mcp.contract.identity import (
    DirectInvocationProvenance, GatewayInternalIdentity, InvocationAuthority,
    PlaneCaller, Principal, PrincipalAccessSubject, PrincipalKind,
    WorkContextMembershipLevel, WorkContextOutputPolicy,
)
from veoveo_mcp.contract.artifacts import ArtifactId, ArtifactPage, ListArtifactsRequest
from pydantic import ValidationError


async def test_list_uses_typed_continuation_and_metadata_across_pages():
    first, second = sorted([str(uuid7()), str(uuid7())], reverse=True)
    now = datetime.now(timezone.utc).isoformat()
    observations = []

    def respond(request):
        assert request.headers["authorization"] == "Bearer fixture-forwarded-identity"
        assert request.url.path == "/artifacts"
        observations.append(dict(request.url.params))
        selected = first if request.url.params.get("cursor") is None else second
        body = {"artifacts": [{"artifactId": selected, "artifactUri": f"artifact://{selected}",
                               "byteLen": 1, "createdAt": now}]}
        if selected == first:
            body["nextCursor"] = first
        return httpx.Response(200, json=body)

    plane = HttpArtifactPlane("https://plane.example", httpx.AsyncClient(transport=httpx.MockTransport(respond)))
    try:
        page = await plane.list(caller(), ListArtifactsRequest(limit=1))
        assert isinstance(page, ArtifactPage)
        assert page.next_cursor == ArtifactId(first)
        final = await plane.list(caller(), ListArtifactsRequest(cursor=page.next_cursor, limit=1))
        assert final.next_cursor is None
        assert [page.artifacts[0].artifact_id, final.artifacts[0].artifact_id] == [first, second]
        assert observations == [{"limit": "1"}, {"cursor": first, "limit": "1"}]
    finally:
        await plane.close()


@pytest.mark.parametrize("body", [
    {"artifacts": [], "next_cursor": str(uuid7())},
    {"artifacts": [], "nextCursor": "not-an-artifact"},
    {"artifacts": [], "unexpected": True},
    {"artifacts": [{"artifactId": str(uuid7()), "artifactUri": f"artifact://{uuid7()}",
                     "byteLen": 1, "createdAt": "2026-10-09T00:00:00Z"}]},
])
async def test_list_refuses_invalid_owner_page_wire(body):
    plane = HttpArtifactPlane("https://plane.example", httpx.AsyncClient(
        transport=httpx.MockTransport(lambda _: httpx.Response(200, json=body))))
    try:
        with pytest.raises(ValidationError):
            await plane.list(caller(), ListArtifactsRequest(limit=1))
    finally:
        await plane.close()


@pytest.mark.parametrize("limit", [True, -1, 65536, 1.5])
def test_list_request_preserves_rust_unsigned_limit_admission(limit):
    with pytest.raises(ValidationError):
        ListArtifactsRequest(limit=limit)


def test_list_wire_omits_absent_fields_and_leaves_service_limit_policy_to_service():
    assert ListArtifactsRequest().model_dump(mode="json") == {}
    assert ListArtifactsRequest(limit=0).model_dump(mode="json") == {"limit": 0}
    assert ListArtifactsRequest(limit=65535).model_dump(mode="json") == {"limit": 65535}
    assert ArtifactPage(artifacts=[]).model_dump(mode="json") == {"artifacts": []}


def caller() -> PlaneCaller:
    now = datetime.now(timezone.utc)
    return PlaneCaller(
        bearer_token="fixture-forwarded-identity",
        identity=GatewayInternalIdentity(
            issuer="veoveo-internal", profile="fixture", server="datasheet",
            jwt_id="test", issued_at=now, not_before=now, expires_at=now,
            actor=Principal(
                id="alice", kind=PrincipalKind.USER, issuer="https://idp.example",
                subject="alice", tenant="tenant",
            ),
            authority=InvocationAuthority(
                work_context="operations", tenant="tenant",
                membership=WorkContextMembershipLevel.CONTRIBUTOR,
                policy_revision="p1",
                output_policy=WorkContextOutputPolicy(
                    owner=PrincipalAccessSubject(kind="principal", id="alice"),
                    initial_grants=(), classification="internal", data_labels=frozenset(),
                ),
                provenance=DirectInvocationProvenance(mode="direct", initiator="alice"),
            ),
        ),
    )


class Chunks(httpx.AsyncByteStream):
    def __init__(self, chunks, wait_after=False):
        self.chunks = chunks
        self.delivered = 0
        self.closed = False
        self.wait_after = wait_after
        self.waiting = asyncio.Event()

    async def __aiter__(self):
        for chunk in self.chunks:
            self.delivered += 1
            yield chunk
        if self.wait_after:
            self.waiting.set()
            await asyncio.Event().wait()

    async def aclose(self):
        self.closed = True


def fixture(chunks, byte_len=None, status=200):
    artifact = str(uuid7())
    uri = f"artifact://{artifact}"
    metadata = {
        "artifactId": artifact, "artifactUri": uri, "filename": "measurements.csv",
        "byteLen": byte_len if byte_len is not None else sum(map(len, chunks.chunks)),
        "createdAt": datetime.now(timezone.utc).isoformat(), "mimeType": "text/csv",
    }

    def respond(request):
        assert request.headers["authorization"] == "Bearer fixture-forwarded-identity"
        assert request.url.host == "plane.example"
        return httpx.Response(status, headers={"x-artifact-metadata": base64.b64encode(json.dumps(metadata).encode()).decode()}, stream=chunks)

    client = httpx.AsyncClient(transport=httpx.MockTransport(respond), follow_redirects=True)
    return HttpArtifactPlane("https://plane.example", client), uri


async def test_stream_and_materialization_preserve_exact_bytes_and_cleanup(tmp_path):
    data = b"timestamp,value\n2026-09-09,42\n"
    source = Chunks([data[:9], data[9:]])
    plane, uri = fixture(source)
    try:
        async with plane.materialize(caller(), uri, max_bytes=1024, expected_sha256=hashlib.sha256(data).hexdigest(), directory=tmp_path) as path:
            assert path.suffix == ".csv"
            assert path.read_bytes() == data
            assert source.closed
        assert not path.exists()
        assert not list(tmp_path.iterdir())
    finally:
        await plane.close()


async def test_declared_multigb_file_is_rejected_before_consuming_when_consumer_limit_is_small():
    source = Chunks([b"must not be consumed"])
    plane, uri = fixture(source, byte_len=10 * 1024**3)
    try:
        with pytest.raises(ArtifactTooLarge):
            async with plane.stream(caller(), uri, max_bytes=1024):
                pytest.fail("oversized stream was exposed")
        assert source.delivered == 0
        assert source.closed
    finally:
        await plane.close()


@pytest.mark.parametrize("operation", ["get", "resolve"])
async def test_repository_enforces_the_consumers_declared_ceiling(operation):
    source = Chunks([b"data"])
    plane, uri = fixture(source)
    repository = ArtifactRepository("https://plane.example", "datasheet")
    await repository.plane.close()
    repository.plane = plane
    target = uri if operation == "resolve" else uri.removeprefix("artifact://")
    try:
        with pytest.raises(ArtifactTooLarge):
            await getattr(repository, operation)(caller(), target, max_bytes=3)
        assert source.delivered == 0
        result = await getattr(repository, operation)(caller(), target, max_bytes=4)
        assert result.bytes_ == b"data"
        assert result.metadata.artifact_uri.startswith("datasheet://")
    finally:
        await repository.close()


async def test_early_exit_closes_the_response_and_yields_bounded_chunks():
    source = Chunks([b"12345678", b"abcdefgh"])
    plane, uri = fixture(source)
    try:
        async with plane.stream(caller(), uri, max_bytes=16, chunk_bytes=4) as download:
            async for chunk in download:
                assert chunk == b"1234"
                break
        assert source.delivered == 1
        assert source.closed
    finally:
        await plane.close()


@pytest.mark.parametrize("declared, chunks", [(3, [b"too long"]), (8, [b"short"])])
async def test_stream_rejects_length_drift(declared, chunks):
    source = Chunks(chunks)
    plane, uri = fixture(source, byte_len=declared)
    try:
        with pytest.raises(ArtifactTransport):
            async with plane.stream(caller(), uri, max_bytes=100, chunk_bytes=2) as download:
                async for _ in download:
                    pass
        assert source.closed
    finally:
        await plane.close()


async def test_failed_digest_never_exposes_a_materialized_file(tmp_path):
    source = Chunks([b"content"])
    plane, uri = fixture(source)
    try:
        with pytest.raises(ArtifactTransport, match="SHA-256"):
            async with plane.materialize(caller(), uri, max_bytes=100, expected_sha256="0" * 64, directory=tmp_path):
                pytest.fail("unverified temporary file was exposed")
        assert not list(tmp_path.iterdir())
    finally:
        await plane.close()


async def test_materialization_cancellation_closes_stream_and_removes_partial_file(tmp_path):
    source = Chunks([b"a" * 1024**2], wait_after=True)
    plane, uri = fixture(source, byte_len=2 * 1024**2)

    async def consume():
        async with plane.materialize(caller(), uri, max_bytes=2 * 1024**2, directory=tmp_path):
            pytest.fail("incomplete file was exposed")

    try:
        task = asyncio.create_task(consume())
        await asyncio.wait_for(source.waiting.wait(), 2)
        partial_files = list(tmp_path.rglob("artifact.csv"))
        assert len(partial_files) == 1
        assert partial_files[0].stat().st_size == 1024**2
        task.cancel()
        with pytest.raises(asyncio.CancelledError):
            await task
        assert source.closed
        assert not list(tmp_path.iterdir())
    finally:
        await plane.close()


async def test_denied_stream_does_not_consume_response_body():
    source = Chunks([b"private backend detail"])
    plane, uri = fixture(source, status=403)
    try:
        with pytest.raises(ArtifactDenied):
            async with plane.stream(caller(), uri, max_bytes=1024):
                pytest.fail("denied bytes were exposed")
        assert source.delivered == 0
        assert source.closed
    finally:
        await plane.close()


@pytest.mark.parametrize("operation", ["get", "resolve"])
async def test_convenience_read_has_a_small_default_bound(operation):
    source = Chunks([b"must not be consumed"])
    plane, uri = fixture(source, byte_len=10 * 1024**3)
    try:
        with pytest.raises(ArtifactTooLarge):
            if operation == "get":
                await plane.get(caller(), uri.removeprefix("artifact://"))
            else:
                await plane.resolve(caller(), uri)
        assert source.delivered == 0
        assert source.closed
    finally:
        await plane.close()


async def test_convenience_read_returns_exact_small_object():
    source = Chunks([b"small", b" object"])
    plane, uri = fixture(source)
    try:
        result = await plane.resolve(caller(), uri)
        assert result.bytes_ == b"small object"
        assert result.metadata.byte_len == 12
    finally:
        await plane.close()


async def test_actual_rust_metadata_is_bound_to_requested_occurrence_before_body_delivery():
    from pathlib import Path
    from veoveo_mcp.contract.artifacts import ArtifactId

    metadata = json.loads((Path(__file__).resolve().parents[3] / "platform/artifacts/contract/tests/fixtures/metadata-output.json").read_text())
    requested = ArtifactId(metadata["artifactId"])
    delivered = Chunks([b"x"])
    foreign = ArtifactId(str(uuid7()))

    def respond(request):
        if request.url.path.endswith("/meta"):
            return httpx.Response(200, json=metadata)
        return httpx.Response(200, headers={"x-artifact-metadata": base64.b64encode(json.dumps(metadata).encode()).decode()}, stream=delivered)

    plane = HttpArtifactPlane("https://plane.example", httpx.AsyncClient(transport=httpx.MockTransport(respond)))
    try:
        assert (await plane.head(caller(), requested)).artifact_id == requested
        with pytest.raises(ArtifactTransport, match="another Artifact"):
            await plane.head(caller(), foreign)
        with pytest.raises(ArtifactTransport, match="another Artifact"):
            await plane.get(caller(), foreign)
        assert delivered.delivered == 0
        assert delivered.closed
    finally:
        await plane.close()
