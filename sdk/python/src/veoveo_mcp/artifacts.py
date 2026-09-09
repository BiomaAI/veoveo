"""HTTP client for the shared artifact plane.

Python port of the Rust `veoveo-artifact-client` crate. Synchronous operations
forward the caller's gateway-signed bearer; asynchronous writes redeem a
separately issued, task-bound write capability. The plane, not this client,
stamps tenant and owner.
"""

from __future__ import annotations

import base64
import asyncio
import hashlib
import json
from collections.abc import AsyncIterator
from contextlib import asynccontextmanager
from dataclasses import dataclass
from pathlib import Path
from tempfile import TemporaryDirectory
from typing import Any

import httpx
from pydantic import TypeAdapter

from .contract.artifacts import (
    ArtifactId,
    ArtifactMetadata,
    ArtifactObject,
    IssueArtifactWriteCapabilityRequest,
    IssuedArtifactWriteCapability,
    PutArtifactRequest,
    RedeemArtifactWriteCapabilityRequest,
)
from .contract.identity import PlaneCaller


class ArtifactPlaneError(Exception):
    pass


class ArtifactNotFound(ArtifactPlaneError):
    def __init__(self) -> None:
        super().__init__("artifact not found")


class ArtifactDenied(ArtifactPlaneError):
    def __init__(self, decision: Any) -> None:
        super().__init__(f"access denied: {decision}")
        self.decision = decision


class ArtifactUnauthenticated(ArtifactPlaneError):
    def __init__(self) -> None:
        super().__init__("unauthenticated")


class ArtifactInvalidRequest(ArtifactPlaneError):
    pass


class ArtifactConflict(ArtifactPlaneError):
    pass


class ArtifactTransport(ArtifactPlaneError):
    pass


class ArtifactTooLarge(ArtifactPlaneError):
    """The consumer's declared byte ceiling was exceeded."""


@dataclass(frozen=True)
class ArtifactStream:
    metadata: ArtifactMetadata
    chunks: AsyncIterator[bytes]

    def __aiter__(self) -> AsyncIterator[bytes]:
        return self.chunks


DEFAULT_OBJECT_READ_BYTES = 8 * 1024 * 1024
DEFAULT_STREAM_CHUNK_BYTES = 1024 * 1024


class HttpArtifactPlane:
    """A plane client bound to one artifact-service base URL."""

    def __init__(self, base_url: str, client: httpx.AsyncClient | None = None) -> None:
        self.base_url = base_url.rstrip("/")
        self._http = client or httpx.AsyncClient(timeout=30.0)

    async def close(self) -> None:
        await self._http.aclose()

    async def issue_write_capability(
        self, caller: PlaneCaller, request: IssueArtifactWriteCapabilityRequest
    ) -> IssuedArtifactWriteCapability:
        response = await self._http.post(
            f"{self.base_url}/artifact-write-capabilities",
            headers={"authorization": f"Bearer {caller.bearer_token}"},
            json=request.model_dump(mode="json"),
        )
        _raise_for_status(response)
        return IssuedArtifactWriteCapability.model_validate(response.json())

    async def redeem_write_capability(
        self,
        secret: str,
        request: RedeemArtifactWriteCapabilityRequest,
        data: bytes,
    ) -> ArtifactMetadata:
        response = await self._http.post(
            f"{self.base_url}/artifact-write-capabilities/"
            f"{request.capability_id}/redeem",
            headers={
                "authorization": f"Bearer {secret}",
                "x-artifact-capability-redeem": json.dumps(request.wire()),
            },
            content=data,
        )
        _raise_for_status(response)
        return ArtifactMetadata.model_validate(response.json())

    async def put(
        self, caller: PlaneCaller, request: PutArtifactRequest, data: bytes
    ) -> ArtifactMetadata:
        response = await self._http.post(
            f"{self.base_url}/artifacts",
            headers={
                "authorization": f"Bearer {caller.bearer_token}",
                "x-artifact-put": json.dumps(request.wire()),
            },
            content=data,
        )
        _raise_for_status(response)
        return ArtifactMetadata.model_validate(response.json())

    async def get(
        self, caller: PlaneCaller, artifact_id: ArtifactId, level: str = "read",
        *, max_bytes: int = DEFAULT_OBJECT_READ_BYTES,
    ) -> ArtifactObject:
        return await self._bounded_object(
            caller, f"/artifacts/{artifact_id}", {"level": level}, max_bytes,
        )

    async def head(
        self, caller: PlaneCaller, artifact_id: ArtifactId
    ) -> ArtifactMetadata:
        response = await self._http.get(
            f"{self.base_url}/artifacts/{artifact_id}/meta",
            headers={"authorization": f"Bearer {caller.bearer_token}"},
        )
        _raise_for_status(response)
        return ArtifactMetadata.model_validate(response.json())

    async def resolve(
        self, caller: PlaneCaller, uri: str, *, max_bytes: int = DEFAULT_OBJECT_READ_BYTES,
    ) -> ArtifactObject:
        return await self._bounded_object(caller, "/resolve", {"uri": uri}, max_bytes)

    async def _bounded_object(
        self, caller: PlaneCaller, path: str, params: dict[str, str], max_bytes: int,
    ) -> ArtifactObject:
        _read_limits(max_bytes, DEFAULT_STREAM_CHUNK_BYTES)
        async with self._http.stream(
            "GET", self.base_url + path, params=params,
            headers={"authorization": f"Bearer {caller.bearer_token}", "accept-encoding": "identity"},
            follow_redirects=False,
        ) as response:
            _raise_for_status(response)
            metadata = _read_metadata(response)
            _metadata_limit(metadata, max_bytes)
            data = bytearray()
            async for chunk in _download_chunks(response, metadata, max_bytes, DEFAULT_STREAM_CHUNK_BYTES, None):
                data.extend(chunk)
            return ArtifactObject(metadata=metadata, bytes=bytes(data))

    @asynccontextmanager
    async def stream(
        self, caller: PlaneCaller, artifact_uri: str, *, max_bytes: int,
        chunk_bytes: int = DEFAULT_STREAM_CHUNK_BYTES, expected_sha256: str | None = None,
    ) -> AsyncIterator[ArtifactStream]:
        """Open an authenticated bounded download of a canonical Artifact URI.

        Consume the full iterator to establish exact length and optional SHA-256.
        Exiting the context closes the connection, including early exit or cancellation.
        """
        _read_limits(max_bytes, chunk_bytes)
        if not artifact_uri.startswith("artifact://"):
            raise ArtifactInvalidRequest("stream requires a canonical artifact:// URI")
        try:
            artifact_id = TypeAdapter(ArtifactId).validate_python(artifact_uri.removeprefix("artifact://"))
        except ValueError as error:
            raise ArtifactInvalidRequest("invalid Artifact URI") from error
        if expected_sha256 is not None and (
            len(expected_sha256) != 64 or any(char not in "0123456789abcdef" for char in expected_sha256)
        ):
            raise ArtifactInvalidRequest("expected_sha256 must be a lowercase SHA-256 digest")
        async with self._http.stream(
            "GET", f"{self.base_url}/artifacts/{artifact_id}/download",
            headers={"authorization": f"Bearer {caller.bearer_token}", "accept-encoding": "identity"},
            follow_redirects=False,
        ) as response:
            _raise_for_status(response)
            if response.status_code != 200:
                raise ArtifactTransport("full artifact download requires HTTP 200")
            metadata = _read_metadata(response)
            if metadata.artifact_id != artifact_id:
                raise ArtifactTransport("download metadata identifies another artifact")
            _metadata_limit(metadata, max_bytes)
            chunks = _download_chunks(response, metadata, max_bytes, chunk_bytes, expected_sha256)
            try:
                yield ArtifactStream(metadata, chunks)
            finally:
                await chunks.aclose()

    @asynccontextmanager
    async def materialize(
        self, caller: PlaneCaller, artifact_uri: str, *, max_bytes: int,
        expected_sha256: str | None = None, directory: Path | None = None,
    ) -> AsyncIterator[Path]:
        """Yield a fully downloaded temporary file and remove it on context exit."""
        with TemporaryDirectory(prefix="veoveo-artifact-", dir=directory) as temporary:
            async with self.stream(caller, artifact_uri, max_bytes=max_bytes, expected_sha256=expected_sha256) as download:
                suffix = Path(download.metadata.filename or "").suffix
                if len(suffix) > 32 or any(not (char.isalnum() or char == ".") for char in suffix):
                    suffix = ""
                path = Path(temporary) / f"artifact{suffix}"
                with path.open("xb") as output:
                    async for chunk in download:
                        pending = asyncio.create_task(asyncio.to_thread(output.write, chunk))
                        try:
                            written = await asyncio.shield(pending)
                        except asyncio.CancelledError:
                            await pending
                            raise
                        if written != len(chunk):
                            raise ArtifactTransport("temporary artifact write was incomplete")
            yield path


class ArtifactRepository:
    """Artifact access for one domain server, presented under its scheme."""

    def __init__(self, service_url: str, scheme: str) -> None:
        self.plane = HttpArtifactPlane(service_url)
        self.scheme = scheme

    async def close(self) -> None:
        await self.plane.close()

    async def put(
        self, caller: PlaneCaller, request: PutArtifactRequest, data: bytes
    ) -> ArtifactMetadata:
        metadata = await self.plane.put(caller, request, data)
        return metadata.presented_under_scheme(self.scheme)

    async def issue_write_capability(
        self, caller: PlaneCaller, request: IssueArtifactWriteCapabilityRequest
    ) -> IssuedArtifactWriteCapability:
        return await self.plane.issue_write_capability(caller, request)

    async def put_with_capability(
        self,
        capability: IssuedArtifactWriteCapability,
        idempotency_key: str,
        request: PutArtifactRequest,
        data: bytes,
    ) -> ArtifactMetadata:
        redemption = RedeemArtifactWriteCapabilityRequest(
            capability_id=capability.capability_id,
            task_id=capability.task_id,
            idempotency_key=idempotency_key,
            artifact=request,
        )
        metadata = await self.plane.redeem_write_capability(
            capability.secret, redemption, data
        )
        return metadata.presented_under_scheme(self.scheme)

    async def get(
        self, caller: PlaneCaller, artifact_id: ArtifactId, *,
        max_bytes: int = DEFAULT_OBJECT_READ_BYTES,
    ) -> ArtifactObject | None:
        try:
            artifact = await self.plane.get(caller, artifact_id, max_bytes=max_bytes)
        except ArtifactNotFound:
            return None
        artifact.metadata = artifact.metadata.presented_under_scheme(self.scheme)
        return artifact

    async def head(
        self, caller: PlaneCaller, artifact_id: ArtifactId
    ) -> ArtifactMetadata | None:
        try:
            metadata = await self.plane.head(caller, artifact_id)
        except ArtifactNotFound:
            return None
        return metadata.presented_under_scheme(self.scheme)

    async def resolve(
        self, caller: PlaneCaller, uri: str, *, max_bytes: int = DEFAULT_OBJECT_READ_BYTES,
    ) -> ArtifactObject:
        artifact = await self.plane.resolve(caller, uri, max_bytes=max_bytes)
        artifact.metadata = artifact.metadata.presented_under_scheme(self.scheme)
        return artifact


def _read_metadata(response: httpx.Response) -> ArtifactMetadata:
    raw = response.headers.get("x-artifact-metadata")
    if raw is None or len(raw) > 64 * 1024:
        raise ArtifactTransport("missing x-artifact-metadata")
    return ArtifactMetadata.model_validate(json.loads(base64.b64decode(raw, validate=True)))


def _read_limits(max_bytes: int, chunk_bytes: int) -> None:
    if type(max_bytes) is not int or max_bytes < 0:
        raise ArtifactInvalidRequest("max_bytes must be a nonnegative byte ceiling")
    if type(chunk_bytes) is not int or not 1 <= chunk_bytes <= 4 * 1024 * 1024:
        raise ArtifactInvalidRequest("chunk_bytes must be between 1 byte and 4 MiB")


def _metadata_limit(metadata: ArtifactMetadata, max_bytes: int) -> None:
    if metadata.byte_len < 0:
        raise ArtifactTransport("negative artifact length")
    if metadata.byte_len > max_bytes:
        raise ArtifactTooLarge(f"artifact requires {metadata.byte_len} bytes; consumer limit is {max_bytes}")


async def _download_chunks(
    response: httpx.Response, metadata: ArtifactMetadata, max_bytes: int,
    chunk_bytes: int, expected_sha256: str | None,
) -> AsyncIterator[bytes]:
    if response.headers.get("content-encoding", "identity") != "identity":
        raise ArtifactTransport("artifact byte streams must use identity encoding")
    declared = response.headers.get("content-length")
    if declared is not None and (not declared.isdecimal() or int(declared) != metadata.byte_len):
        raise ArtifactTransport("artifact Content-Length does not match metadata")
    total = 0
    digest = hashlib.sha256() if expected_sha256 else None
    async for chunk in response.aiter_raw(chunk_bytes):
        total += len(chunk)
        if total > max_bytes:
            raise ArtifactTooLarge(f"artifact stream exceeded the {max_bytes}-byte consumer limit")
        if total > metadata.byte_len:
            raise ArtifactTransport("artifact stream exceeded its declared length")
        if digest is not None:
            digest.update(chunk)
        yield chunk
    if total != metadata.byte_len:
        raise ArtifactTransport("artifact stream ended before its declared length")
    if digest is not None and digest.hexdigest() != expected_sha256:
        raise ArtifactTransport("artifact SHA-256 verification failed")


def _raise_for_status(response: httpx.Response) -> None:
    if response.is_success:
        return
    body = response.text if response.is_stream_consumed else f"HTTP {response.status_code}"
    if response.status_code == 404:
        raise ArtifactNotFound()
    if response.status_code == 401:
        raise ArtifactUnauthenticated()
    if response.status_code == 400:
        raise ArtifactInvalidRequest(body)
    if response.status_code == 409:
        raise ArtifactConflict(body)
    if response.status_code == 403:
        decision = response.headers.get("x-artifact-decision")
        raise ArtifactDenied(decision if decision is not None else "DenyNeedToKnow")
    raise ArtifactTransport(f"{response.status_code}: {body}")
