"""Authoritative shared camera product and viewer authorization fixture runtime."""

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
    CameraRegion,
    CloseLiveViewRequest,
    CloseLiveViewResult,
    FixtureState,
    LiveViewConnection,
    LiveViewState,
    MediaEndpoint,
    OpenLiveViewRequest,
    ProductLifecycle,
    RenewLiveViewRequest,
    StreamProduct,
    ViewLifecycle,
)


SESSION_ID = "authoritative-session"
CAMERA_ID = "operator-fixed"
STREAM_PRODUCT_ID = "camera-product-0"


@dataclass(slots=True)
class _Authorization:
    wire: LiveViewState
    token_hash: bytes
    generation: int


class FixtureRuntime:
    """Own one continuous product and independent viewer authorizations."""

    def __init__(self, config: Config) -> None:
        self._config = config
        self._lock = asyncio.Lock()
        self._authorizations: dict[str, _Authorization] = {}
        self._frame_sequence = 1
        self._camera = CameraDescriptor(
            session_id=SESSION_ID,
            camera_id=CAMERA_ID,
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
            for authorization in self._authorizations.values():
                if (
                    authorization.wire.lifecycle is not ViewLifecycle.CLOSED
                    and authorization.wire.viewer_actor == actor
                    and authorization.wire.owner == owner
                    and authorization.wire.viewer_instance_id == request.viewer_instance_id
                    and authorization.wire.camera_id == request.camera_id
                ):
                    return self._rotate(authorization, now)
            live_view_id = f"view-{uuid4()}"
            token = _token()
            wire = LiveViewState(
                live_view_id=live_view_id,
                resource_uri=(
                    f"anonymous-simulation://session/{SESSION_ID}/live-view/{live_view_id}"
                ),
                session_id=SESSION_ID,
                camera_id=CAMERA_ID,
                stream_product_id=STREAM_PRODUCT_ID,
                owner=owner,
                viewer_actor=actor,
                viewer_instance_id=request.viewer_instance_id,
                lifecycle=ViewLifecycle.READY,
                width_px=self._camera.width_px,
                height_px=self._camera.height_px,
                coded_width_px=self._camera.width_px,
                coded_height_px=self._camera.height_px,
                source_region=CameraRegion(
                    camera_id=CAMERA_ID,
                    x_px=0,
                    y_px=0,
                    width_px=self._camera.width_px,
                    height_px=self._camera.height_px,
                ),
                frame_rate_millihertz=self._camera.frame_rate_millihertz,
                connected_viewers=0,
                endpoint=MediaEndpoint(stream_url=self._config.public_stream_url),
                created_at=now,
                expires_at=now
                + timedelta(seconds=self._config.authorization_seconds),
            )
            authorization = _Authorization(
                wire=wire,
                token_hash=_hash(token),
                generation=1,
            )
            self._authorizations[live_view_id] = authorization
            self._arm_expiry(live_view_id, authorization.generation, wire.expires_at)
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
            authorization = self._owned(
                actor, owner, request.live_view_id, request.viewer_instance_id
            )
            if authorization.wire.lifecycle is ViewLifecycle.CLOSED:
                raise ValueError("live view is closed or expired")
            connection = self._rotate(authorization, now)
            self._arm_expiry(
                request.live_view_id,
                authorization.generation,
                authorization.wire.expires_at,
            )
            return connection

    async def close(
        self,
        actor: str,
        owner: str,
        request: CloseLiveViewRequest,
    ) -> CloseLiveViewResult:
        self._require_session(request.session_id)
        async with self._lock:
            authorization = self._owned(
                actor, owner, request.live_view_id, request.viewer_instance_id
            )
            self._close(authorization)
            return CloseLiveViewResult(
                resource_uri=authorization.wire.resource_uri,
                closed=True,
            )

    async def authorize_stream(self, live_view_id: str, token: str) -> LiveViewState:
        now = _now()
        async with self._lock:
            self._expire(now)
            authorization = self._authorizations.get(live_view_id)
            if (
                authorization is None
                or authorization.wire.lifecycle is ViewLifecycle.CLOSED
                or authorization.wire.connected_viewers != 0
                or not secrets.compare_digest(authorization.token_hash, _hash(token))
            ):
                raise ValueError("stream authorization failed")
            authorization.wire = authorization.wire.model_copy(
                update={"lifecycle": ViewLifecycle.LIVE, "connected_viewers": 1}
            )
            return authorization.wire

    async def finish_stream(self, live_view_id: str) -> None:
        async with self._lock:
            authorization = self._authorizations.get(live_view_id)
            if (
                authorization is not None
                and authorization.wire.lifecycle is not ViewLifecycle.CLOSED
            ):
                authorization.wire = authorization.wire.model_copy(
                    update={"lifecycle": ViewLifecycle.READY, "connected_viewers": 0}
                )

    async def fixture_state(self) -> FixtureState:
        async with self._lock:
            self._expire(_now())
            active = [
                authorization.wire
                for authorization in self._authorizations.values()
                if authorization.wire.lifecycle is not ViewLifecycle.CLOSED
            ]
            product = StreamProduct(
                stream_product_id=STREAM_PRODUCT_ID,
                camera_regions=(
                    CameraRegion(
                        camera_id=CAMERA_ID,
                        x_px=0,
                        y_px=0,
                        width_px=self._camera.width_px,
                        height_px=self._camera.height_px,
                    ),
                ),
                coded_width_px=self._camera.width_px,
                coded_height_px=self._camera.height_px,
                lifecycle=ProductLifecycle.READY,
                active_viewers=len(active),
                connected_viewers=sum(view.connected_viewers for view in active),
                nvenc_sessions=1,
                encoded_frames=self._frame_sequence,
                source_to_render_samples=self._frame_sequence,
            )
            return FixtureState(
                session_id=SESSION_ID,
                cameras=(self._camera,),
                stream_products=(product,),
            )

    async def close_runtime(self) -> None:
        async with self._lock:
            for authorization in self._authorizations.values():
                self._close(authorization)

    def _owned(
        self, actor: str, owner: str, live_view_id: str, viewer_instance_id: str
    ) -> _Authorization:
        authorization = self._authorizations.get(live_view_id)
        if (
            authorization is None
            or authorization.wire.viewer_actor != actor
            or authorization.wire.owner != owner
            or authorization.wire.viewer_instance_id != viewer_instance_id
        ):
            raise ValueError("live-view ownership does not match the caller")
        return authorization

    def _rotate(
        self, authorization: _Authorization, now: datetime
    ) -> LiveViewConnection:
        token = _token()
        authorization.token_hash = _hash(token)
        authorization.generation += 1
        authorization.wire = authorization.wire.model_copy(
            update={
                "expires_at": now
                + timedelta(seconds=self._config.authorization_seconds),
            }
        )
        return LiveViewConnection(stream=authorization.wire, access_token=token)

    def _arm_expiry(
        self, live_view_id: str, generation: int, expires_at: datetime
    ) -> None:
        async def expire() -> None:
            delay = max(0.0, (expires_at - _now()).total_seconds())
            await asyncio.sleep(delay)
            async with self._lock:
                authorization = self._authorizations.get(live_view_id)
                if (
                    authorization is not None
                    and authorization.generation == generation
                    and authorization.wire.expires_at <= _now()
                ):
                    self._close(authorization)

        asyncio.create_task(expire(), name=f"expire-{live_view_id}")

    def _expire(self, now: datetime) -> None:
        for authorization in self._authorizations.values():
            if authorization.wire.expires_at <= now:
                self._close(authorization)

    @staticmethod
    def _close(authorization: _Authorization) -> None:
        authorization.token_hash = b"\0" * 32
        authorization.generation += 1
        authorization.wire = authorization.wire.model_copy(
            update={
                "lifecycle": ViewLifecycle.CLOSED,
                "connected_viewers": 0,
                "expires_at": _now(),
            }
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
