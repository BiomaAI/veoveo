"""HTTP client for the shared artifact plane.

Python port of the Rust `veoveo-artifact-client` crate. Synchronous operations
forward the caller's gateway-signed bearer; asynchronous writes redeem a
separately issued, task-bound write capability. The plane, not this client,
stamps tenant and owner.
"""

from __future__ import annotations

import base64
import json
from typing import Any

import httpx

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
        self, caller: PlaneCaller, artifact_id: ArtifactId, level: str = "read"
    ) -> ArtifactObject:
        response = await self._http.get(
            f"{self.base_url}/artifacts/{artifact_id}",
            params={"level": level},
            headers={"authorization": f"Bearer {caller.bearer_token}"},
        )
        _raise_for_status(response)
        return _read_object(response)

    async def head(
        self, caller: PlaneCaller, artifact_id: ArtifactId
    ) -> ArtifactMetadata:
        response = await self._http.get(
            f"{self.base_url}/artifacts/{artifact_id}/meta",
            headers={"authorization": f"Bearer {caller.bearer_token}"},
        )
        _raise_for_status(response)
        return ArtifactMetadata.model_validate(response.json())

    async def resolve(self, caller: PlaneCaller, uri: str) -> ArtifactObject:
        response = await self._http.get(
            f"{self.base_url}/resolve",
            params={"uri": uri},
            headers={"authorization": f"Bearer {caller.bearer_token}"},
        )
        _raise_for_status(response)
        return _read_object(response)


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
        self, caller: PlaneCaller, artifact_id: ArtifactId
    ) -> ArtifactObject | None:
        try:
            artifact = await self.plane.get(caller, artifact_id)
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

    async def resolve(self, caller: PlaneCaller, uri: str) -> ArtifactObject:
        artifact = await self.plane.resolve(caller, uri)
        artifact.metadata = artifact.metadata.presented_under_scheme(self.scheme)
        return artifact


def _read_object(response: httpx.Response) -> ArtifactObject:
    raw = response.headers.get("x-artifact-metadata")
    if raw is None:
        raise ArtifactTransport("missing x-artifact-metadata")
    metadata = ArtifactMetadata.model_validate(json.loads(base64.b64decode(raw)))
    return ArtifactObject(metadata=metadata, bytes=response.content)


def _raise_for_status(response: httpx.Response) -> None:
    if response.is_success:
        return
    body = response.text
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
