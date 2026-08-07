"""Authoritative camera product and ephemeral viewer-lease fixture runtime."""

from __future__ import annotations

import asyncio
import base64
import hashlib
import secrets
from dataclasses import dataclass
from datetime import datetime, timedelta, timezone
from uuid import uuid4

from .config import Config
from .contract import (
    CameraDescriptor,
    CameraHealth,
    CloseLiveViewRequest,
    CloseLiveViewResult,
    FixtureState,
    LeaseLifecycle,
    LiveViewConnection,
    OpenLiveViewRequest,
    ProductLifecycle,
    RenewLiveViewRequest,
    StreamProduct,
    ViewerLease,
)


SESSION_ID = "authoritative-session"
CAMERA_ID = "operator-fixed"
PRODUCT_ID = "operator-fixed-h264"


@dataclass(slots=True)
class _Lease:
    wire: ViewerLease
    token_hash: bytes
    generation: int


class FixtureRuntime:
    """Own one stable product while keeping every viewer lease ephemeral."""

    def __init__(self, config: Config) -> None:
        self._config = config
        self._lock = asyncio.Lock()
        self._leases: dict[str, _Lease] = {}
        self._frame_sequence = 1
        self._camera = CameraDescriptor(
            session_id=SESSION_ID,
            camera_id=CAMERA_ID,
            stream_product_id=PRODUCT_ID,
            width_px=1280,
            height_px=720,
            frame_rate_millihertz=30_000,
            health=CameraHealth.READY,
            revision=1,
        )

    async def list_live_cameras(self, session_id: str) -> tuple[CameraDescriptor, ...]:
        self._require_session(session_id)
        return (self._camera,)

    async def open(
        self,
        actor: str,
        owner: str,
        request: OpenLiveViewRequest,
    ) -> LiveViewConnection:
        self._require_session(request.session_id)
        if request.camera_id != CAMERA_ID:
            raise ValueError("live camera was not found")
        now = _now()
        async with self._lock:
            self._expire(now)
            for lease in self._leases.values():
                if (
                    lease.wire.lifecycle is not LeaseLifecycle.CLOSED
                    and lease.wire.viewer_actor == actor
                    and lease.wire.owner == owner
                    and lease.wire.viewer_instance_id == request.viewer_instance_id
                    and lease.wire.camera_id == request.camera_id
                ):
                    return self._rotate(lease, now)
            if self._active_count() >= self._config.maximum_viewers:
                raise ValueError("live-view viewer capacity is exhausted")
            live_view_id = f"view-{uuid4()}"
            token = _token()
            wire = ViewerLease(
                live_view_id=live_view_id,
                resource_uri=(
                    f"anonymous-simulation://session/{SESSION_ID}/live-view/{live_view_id}"
                ),
                session_id=SESSION_ID,
                camera_id=CAMERA_ID,
                stream_product_id=PRODUCT_ID,
                owner=owner,
                viewer_actor=actor,
                viewer_instance_id=request.viewer_instance_id,
                lifecycle=LeaseLifecycle.READY,
                signaling_url=self._config.public_signaling_url,
                media_host=self._config.public_media_host,
                media_port=self._config.public_media_port,
                created_at=now,
                expires_at=now + timedelta(seconds=self._config.lease_seconds),
            )
            lease = _Lease(wire=wire, token_hash=_hash(token), generation=1)
            self._leases[live_view_id] = lease
            self._arm_expiry(live_view_id, lease.generation, wire.expires_at)
            return LiveViewConnection(stream=wire, access_token=token)

    async def renew(
        self,
        actor: str,
        owner: str,
        request: RenewLiveViewRequest,
    ) -> LiveViewConnection:
        self._require_session(request.session_id)
        now = _now()
        async with self._lock:
            self._expire(now)
            lease = self._owned(actor, owner, request.live_view_id, request.viewer_instance_id)
            if lease.wire.lifecycle is LeaseLifecycle.CLOSED:
                raise ValueError("live view is closed or expired")
            connection = self._rotate(lease, now)
            self._arm_expiry(request.live_view_id, lease.generation, lease.wire.expires_at)
            return connection

    async def close(
        self,
        actor: str,
        owner: str,
        request: CloseLiveViewRequest,
    ) -> CloseLiveViewResult:
        self._require_session(request.session_id)
        async with self._lock:
            lease = self._owned(actor, owner, request.live_view_id, request.viewer_instance_id)
            self._close(lease)
            return CloseLiveViewResult(resource_uri=lease.wire.resource_uri, closed=True)

    async def authorize_signaling(self, live_view_id: str, token: str) -> ViewerLease:
        now = _now()
        async with self._lock:
            self._expire(now)
            lease = self._leases.get(live_view_id)
            if (
                lease is None
                or lease.wire.lifecycle is LeaseLifecycle.CLOSED
                or not secrets.compare_digest(lease.token_hash, _hash(token))
            ):
                raise ValueError("signaling authorization failed")
            lease.wire = lease.wire.model_copy(update={"lifecycle": LeaseLifecycle.LIVE})
            return lease.wire

    async def fixture_state(self) -> FixtureState:
        async with self._lock:
            self._expire(_now())
            active = self._active_count()
            product = StreamProduct(
                stream_product_id=PRODUCT_ID,
                camera_id=CAMERA_ID,
                lifecycle=ProductLifecycle.READY,
                connected_viewers=active,
                last_frame_sequence=self._frame_sequence,
            )
            return FixtureState(
                session_id=SESSION_ID,
                cameras=(self._camera,),
                stream_products=(product,),
                active_viewer_leases=active,
            )

    async def close_runtime(self) -> None:
        async with self._lock:
            for lease in self._leases.values():
                self._close(lease)

    def _owned(
        self, actor: str, owner: str, live_view_id: str, viewer_instance_id: str
    ) -> _Lease:
        lease = self._leases.get(live_view_id)
        if (
            lease is None
            or lease.wire.viewer_actor != actor
            or lease.wire.owner != owner
            or lease.wire.viewer_instance_id != viewer_instance_id
        ):
            raise ValueError("live-view ownership does not match the caller")
        return lease

    def _rotate(self, lease: _Lease, now: datetime) -> LiveViewConnection:
        token = _token()
        lease.token_hash = _hash(token)
        lease.generation += 1
        lease.wire = lease.wire.model_copy(
            update={
                "expires_at": now + timedelta(seconds=self._config.lease_seconds),
            }
        )
        return LiveViewConnection(stream=lease.wire, access_token=token)

    def _arm_expiry(self, live_view_id: str, generation: int, expires_at: datetime) -> None:
        async def expire() -> None:
            delay = max(0.0, (expires_at - _now()).total_seconds())
            await asyncio.sleep(delay)
            async with self._lock:
                lease = self._leases.get(live_view_id)
                if (
                    lease is not None
                    and lease.generation == generation
                    and lease.wire.expires_at <= _now()
                ):
                    self._close(lease)

        asyncio.create_task(expire(), name=f"expire-{live_view_id}")

    def _expire(self, now: datetime) -> None:
        for lease in self._leases.values():
            if lease.wire.expires_at <= now:
                self._close(lease)

    def _active_count(self) -> int:
        return sum(
            lease.wire.lifecycle is not LeaseLifecycle.CLOSED
            for lease in self._leases.values()
        )

    @staticmethod
    def _close(lease: _Lease) -> None:
        lease.token_hash = b"\0" * 32
        lease.generation += 1
        lease.wire = lease.wire.model_copy(
            update={"lifecycle": LeaseLifecycle.CLOSED, "expires_at": _now()}
        )

    @staticmethod
    def _require_session(session_id: str) -> None:
        if session_id != SESSION_ID:
            raise ValueError("simulation session was not found")


def _now() -> datetime:
    return datetime.now(timezone.utc)


def _token() -> str:
    return base64.urlsafe_b64encode(secrets.token_bytes(32)).rstrip(b"=").decode()


def _hash(token: str) -> bytes:
    return hashlib.sha256(token.encode()).digest()
