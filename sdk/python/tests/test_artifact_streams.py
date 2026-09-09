import asyncio
import base64
import hashlib
import json
from datetime import datetime, timezone

import httpx
import pytest
import uuid_extensions

from veoveo_mcp.artifacts import (
    ArtifactDenied, ArtifactRepository, ArtifactTooLarge, ArtifactTransport, HttpArtifactPlane,
)
from veoveo_mcp.contract.identity import PlaneCaller


def caller():
    now = datetime.now(timezone.utc).isoformat()
    return PlaneCaller.model_validate({
        "bearer_token": "fixture-forwarded-identity",
        "identity": {
            "issuer": "veoveo-internal", "profile": "fixture", "server": "datasheet",
            "jwt_id": "test", "issued_at": now, "not_before": now, "expires_at": now,
            "actor": {"id": "alice", "kind": "user", "issuer": "https://idp.example", "subject": "alice", "tenant": "tenant"},
            "authority": {"work_context": "operations", "tenant": "tenant", "membership": "contributor", "policy_revision": "p1",
                "output_policy": {"owner": {"kind": "principal", "id": "alice"}, "initial_grants": [], "classification": "internal", "data_labels": []},
                "provenance": {"mode": "direct", "initiator": "alice"}},
        },
    })


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
    artifact = str(uuid_extensions.uuid7())
    uri = f"artifact://{artifact}"
    metadata = {
        "artifact_id": artifact, "artifact_uri": uri, "filename": "measurements.csv",
        "byte_len": byte_len if byte_len is not None else sum(map(len, chunks.chunks)),
        "created_at": datetime.now(timezone.utc).isoformat(), "mime_type": "text/csv",
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
