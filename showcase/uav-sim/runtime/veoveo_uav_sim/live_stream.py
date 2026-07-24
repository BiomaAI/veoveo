from __future__ import annotations

import asyncio
import ctypes
import hashlib
import logging
import secrets
import threading
from dataclasses import dataclass, field
from datetime import datetime, timedelta, timezone
from typing import Any, Callable

from aiohttp import ClientSession, WSMsgType, web

from .config import FollowCameraConfig, LiveStreamConfig


LOGGER = logging.getLogger("veoveo.uav_sim.live_stream")
AUTH_PROTOCOL_PREFIX = "authorization.bearer."


def _timestamp(value: datetime) -> str:
    return value.astimezone(timezone.utc).isoformat().replace("+00:00", "Z")


def verify_nvidia_video_stack() -> None:
    try:
        cuda = ctypes.CDLL("libcuda.so.1")
        nvenc = ctypes.CDLL("libnvidia-encode.so.1")
    except OSError as error:
        raise RuntimeError(
            "NVIDIA CUDA and NVENC libraries are required for UAV live streaming"
        ) from error
    cuda.cuInit.argtypes = [ctypes.c_uint]
    cuda.cuInit.restype = ctypes.c_int
    cuda.cuDeviceGetCount.argtypes = [ctypes.POINTER(ctypes.c_int)]
    cuda.cuDeviceGetCount.restype = ctypes.c_int
    device_count = ctypes.c_int()
    if cuda.cuInit(0) != 0 or cuda.cuDeviceGetCount(ctypes.byref(device_count)) != 0:
        raise RuntimeError("NVIDIA CUDA driver initialization failed")
    if device_count.value < 1:
        raise RuntimeError("an accessible NVIDIA CUDA device is required")
    if not hasattr(nvenc, "NvEncodeAPICreateInstance"):
        raise RuntimeError("the NVIDIA NVENC API is unavailable")


@dataclass(slots=True)
class LiveStreamLease:
    stream_id: str
    access_token: str
    token_digest: bytes
    expires_at: datetime
    signaling_connection_ids: set[str] = field(default_factory=set)
    revoked: bool = False


@dataclass(frozen=True, slots=True)
class AuthenticatedSignalingConnection:
    stream_id: str
    connection_id: str


class LiveStreamLeaseManager:
    def __init__(
        self,
        ttl_seconds: int,
        on_change: Callable[[str, int], None],
    ) -> None:
        self._ttl = timedelta(seconds=ttl_seconds)
        self._on_change = on_change
        self._lock = threading.Lock()
        self._lease: LiveStreamLease | None = None

    def open(self, stream_id: str) -> dict[str, str]:
        with self._lock:
            self._purge_expired()
            if self._lease is not None and not self._lease.revoked:
                raise RuntimeError("the follow-camera live stream is already leased")
            token = secrets.token_urlsafe(32)
            expires_at = datetime.now(timezone.utc) + self._ttl
            self._lease = LiveStreamLease(
                stream_id=stream_id,
                access_token=token,
                token_digest=self._digest(token),
                expires_at=expires_at,
            )
        self._notify()
        return {
            "stream_id": stream_id,
            "access_token": token,
            "expires_at": _timestamp(expires_at),
        }

    def renew(self, stream_id: str) -> dict[str, str]:
        with self._lock:
            lease = self._require(stream_id)
            lease.expires_at = datetime.now(timezone.utc) + self._ttl
            expires_at = lease.expires_at
            token = lease.access_token
        self._notify()
        return {
            "stream_id": stream_id,
            "access_token": token,
            "expires_at": _timestamp(expires_at),
        }

    def close(self, stream_id: str) -> None:
        with self._lock:
            lease = self._require(stream_id)
            lease.revoked = True
        self._notify()

    def authorize(self, token: str) -> AuthenticatedSignalingConnection:
        with self._lock:
            self._purge_expired()
            lease = self._lease
            if (
                lease is None
                or lease.revoked
                or not secrets.compare_digest(lease.token_digest, self._digest(token))
            ):
                raise PermissionError("live-stream lease is invalid or unavailable")
            connection = AuthenticatedSignalingConnection(
                stream_id=lease.stream_id,
                connection_id=secrets.token_urlsafe(16),
            )
            lease.signaling_connection_ids.add(connection.connection_id)
        self._notify()
        return connection

    def disconnect(self, connection: AuthenticatedSignalingConnection) -> None:
        with self._lock:
            if (
                self._lease is not None
                and self._lease.stream_id == connection.stream_id
            ):
                self._lease.signaling_connection_ids.discard(
                    connection.connection_id
                )
        self._notify()

    def active(self, stream_id: str) -> bool:
        with self._lock:
            self._purge_expired()
            return (
                self._lease is not None
                and self._lease.stream_id == stream_id
                and not self._lease.revoked
            )

    def public_state(self) -> tuple[str, int]:
        with self._lock:
            self._purge_expired()
            connected = int(
                self._lease is not None
                and bool(self._lease.signaling_connection_ids)
                and not self._lease.revoked
            )
        return ("live" if connected else "ready", connected)

    def _require(self, stream_id: str) -> LiveStreamLease:
        self._purge_expired()
        if (
            self._lease is None
            or self._lease.stream_id != stream_id
            or self._lease.revoked
        ):
            raise ValueError(f"unknown live stream {stream_id!r}")
        return self._lease

    def _purge_expired(self) -> None:
        if (
            self._lease is not None
            and self._lease.expires_at <= datetime.now(timezone.utc)
        ):
            self._lease.revoked = True

    def _notify(self) -> None:
        lifecycle, connected = self.public_state()
        self._on_change(lifecycle, connected)

    @staticmethod
    def _digest(token: str) -> bytes:
        return hashlib.sha256(token.encode("utf-8")).digest()


class LiveStreamSignalingProxy:
    def __init__(
        self,
        config: LiveStreamConfig,
        leases: LiveStreamLeaseManager,
    ) -> None:
        self._config = config
        self._leases = leases
        self._thread: threading.Thread | None = None
        self._loop: asyncio.AbstractEventLoop | None = None
        self._runner: web.AppRunner | None = None
        self._started = threading.Event()
        self._error: BaseException | None = None

    def start(self) -> None:
        self._thread = threading.Thread(
            target=self._run,
            name="uav-live-stream-signaling",
            daemon=True,
        )
        self._thread.start()
        if not self._started.wait(30.0):
            raise TimeoutError("UAV live-stream signaling proxy did not start")
        if self._error is not None:
            raise RuntimeError("UAV live-stream signaling proxy failed") from self._error

    def close(self) -> None:
        if self._loop is not None and self._runner is not None:
            future = asyncio.run_coroutine_threadsafe(
                self._runner.cleanup(), self._loop
            )
            future.result(timeout=30.0)
            self._loop.call_soon_threadsafe(self._loop.stop)
        if self._thread is not None:
            self._thread.join(timeout=30.0)

    def _run(self) -> None:
        try:
            self._loop = asyncio.new_event_loop()
            asyncio.set_event_loop(self._loop)
            application = web.Application(client_max_size=64 * 1024)
            application.add_routes(
                [
                    web.get(self._config.signaling_path, self._websocket),
                    web.get(
                        f"{self._config.signaling_path}/{{signaling_suffix:.*}}",
                        self._websocket,
                    ),
                ]
            )
            self._runner = web.AppRunner(application, access_log=None)
            self._loop.run_until_complete(self._runner.setup())
            site = web.TCPSite(
                self._runner,
                self._config.proxy_host,
                self._config.proxy_port,
            )
            self._loop.run_until_complete(site.start())
            self._started.set()
            self._loop.run_forever()
        except BaseException as error:
            self._error = error
            self._started.set()

    async def _websocket(self, request: web.Request) -> web.StreamResponse:
        offered_protocols = [
            value.strip()
            for value in request.headers.get("Sec-WebSocket-Protocol", "").split(",")
            if value.strip()
        ]
        token = _authorization_token(offered_protocols)
        if token is None:
            raise web.HTTPUnauthorized(text="live-stream bearer protocol is required")
        try:
            connection = self._leases.authorize(token)
        except PermissionError as error:
            raise web.HTTPForbidden(text=str(error)) from error

        upstream_protocols = [
            protocol
            for protocol in offered_protocols
            if not protocol.startswith(AUTH_PROTOCOL_PREFIX)
        ]
        if f"x-nv-sessionid.{connection.stream_id}" not in upstream_protocols:
            self._leases.disconnect(connection)
            raise web.HTTPForbidden(
                text="live-stream session protocol does not match the lease"
            )
        upstream_path = request.rel_url.raw_path.removeprefix(
            self._config.signaling_path
        )
        if not upstream_path:
            upstream_path = "/"
        elif not upstream_path.startswith("/"):
            raise web.HTTPNotFound()
        upstream_url = (
            f"ws://127.0.0.1:{self._config.signal_port}{upstream_path}"
        )
        if request.rel_url.raw_query_string:
            upstream_url = (
                f"{upstream_url}?{request.rel_url.raw_query_string}"
            )
        try:
            async with ClientSession() as session:
                upstream = await session.ws_connect(
                    upstream_url,
                    protocols=upstream_protocols,
                    max_msg_size=16 * 1024 * 1024,
                )
                selected = [upstream.protocol] if upstream.protocol else []
                downstream = web.WebSocketResponse(
                    protocols=selected,
                    max_msg_size=16 * 1024 * 1024,
                    heartbeat=20.0,
                )
                await downstream.prepare(request)
                LOGGER.info(
                    "NVIDIA WebRTC signaling connected: stream=%s",
                    connection.stream_id,
                )
                await self._bridge(connection, downstream, upstream)
                return downstream
        except BaseException:
            self._leases.disconnect(connection)
            raise

    async def _bridge(
        self,
        connection: AuthenticatedSignalingConnection,
        downstream: web.WebSocketResponse,
        upstream: Any,
    ) -> None:
        downstream_task = asyncio.create_task(_forward_websocket(downstream, upstream))
        upstream_task = asyncio.create_task(_forward_websocket(upstream, downstream))
        lease_task = asyncio.create_task(self._watch_lease(connection.stream_id))
        tasks = {downstream_task, upstream_task, lease_task}
        try:
            _, pending = await asyncio.wait(
                tasks, return_when=asyncio.FIRST_COMPLETED
            )
            for task in pending:
                task.cancel()
            await asyncio.gather(*pending, return_exceptions=True)
        finally:
            await downstream.close()
            await upstream.close()
            self._leases.disconnect(connection)
            LOGGER.info(
                "NVIDIA WebRTC signaling closed: stream=%s",
                connection.stream_id,
            )

    async def _watch_lease(self, stream_id: str) -> None:
        while self._leases.active(stream_id):
            await asyncio.sleep(1.0)


def _authorization_token(protocols: list[str]) -> str | None:
    for protocol in protocols:
        if protocol.startswith(AUTH_PROTOCOL_PREFIX):
            token = protocol.removeprefix(AUTH_PROTOCOL_PREFIX)
            return token or None
    return None


async def _forward_websocket(source: Any, destination: Any) -> None:
    async for message in source:
        if message.type == WSMsgType.TEXT:
            await destination.send_str(message.data)
        elif message.type == WSMsgType.BINARY:
            await destination.send_bytes(message.data)
        elif message.type in {WSMsgType.CLOSE, WSMsgType.CLOSED}:
            return
        elif message.type == WSMsgType.ERROR:
            raise RuntimeError("NVIDIA WebRTC signaling websocket failed")


def live_stream_state(
    follow_camera: FollowCameraConfig,
    lifecycle: str = "starting",
    connected_viewers: int = 0,
) -> dict[str, object]:
    return {
        "lifecycle": lifecycle,
        "source": "follow_camera",
        "codec": "h264",
        "hardware_encoder": "nvidia_nvenc",
        "width": follow_camera.width,
        "height": follow_camera.height,
        "fps": follow_camera.fps,
        "connected_viewers": connected_viewers,
    }
