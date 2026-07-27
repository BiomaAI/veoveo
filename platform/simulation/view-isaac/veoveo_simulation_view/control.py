from __future__ import annotations

import json
import logging
import queue
import re
import secrets
import threading
from dataclasses import dataclass
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import Any

from .config import RendererConfig
from .contracts import (
    CameraBinding,
    ContractError,
    PoseSourceBinding,
    SceneBinding,
    SessionBinding,
    StreamBinding,
)
from .scene import ArtifactMaterializer


LOGGER = logging.getLogger("veoveo.simulation_view.control")
MAXIMUM_BODY_BYTES = 4 * 1024 * 1024
SESSION = r"(?P<session>[A-Za-z0-9_.-]{1,128})"
CAMERA = r"(?P<camera>[A-Za-z0-9_.-]{1,128})"
STREAM = r"(?P<stream>[A-Za-z0-9_.-]{1,128})"
ARTIFACT_PATH = re.compile(
    r"^/v1/artifacts/sha256/"
    r"(?P<digest>[0-9a-f]{64})\.(?P<format>usd|usdz|glb|gltf)$"
)

SESSION_PATH = re.compile(rf"^/v1/sessions/{SESSION}$")
SCENE_PATH = re.compile(rf"^/v1/sessions/{SESSION}/scene$")
POSE_PATH = re.compile(rf"^/v1/sessions/{SESSION}/pose-source$")
CAMERA_PATH = re.compile(
    rf"^/v1/sessions/{SESSION}/cameras/{CAMERA}$"
)
STREAM_PATH = re.compile(
    rf"^/v1/sessions/{SESSION}/streams/{STREAM}$"
)


@dataclass(frozen=True, slots=True)
class Readiness:
    ready: bool = False
    profile: str = "veoveo.io/simulation-view-renderer/isaac-rtx/v1"
    hardware_accelerated: bool = False
    nvidia: bool = False
    render_product_ready: bool = False
    nvenc_ready: bool = False
    visible_non_stale_frame: bool = False

    def response(self) -> dict[str, object]:
        return {
            "ready": self.ready,
            "profile": self.profile,
            "hardwareAccelerated": self.hardware_accelerated,
            "nvidia": self.nvidia,
            "renderProductReady": self.render_product_ready,
            "nvencReady": self.nvenc_ready,
            "visibleNonStaleFrame": self.visible_non_stale_frame,
        }


class ReadinessSlot:
    def __init__(self) -> None:
        self._lock = threading.Lock()
        self._value = Readiness()

    def get(self) -> Readiness:
        with self._lock:
            return self._value

    def set(self, value: Readiness) -> None:
        with self._lock:
            self._value = value


@dataclass(slots=True)
class CommandResult:
    status: int
    body: dict[str, object] | None = None


class ControlCommand:
    def __init__(
        self,
        operation: str,
        session_id: str,
        resource_id: str | None,
        value: object | None,
    ) -> None:
        self.operation = operation
        self.session_id = session_id
        self.resource_id = resource_id
        self.value = value
        self._event = threading.Event()
        self._result: CommandResult | None = None

    def complete(self, result: CommandResult) -> None:
        self._result = result
        self._event.set()

    def wait(self, timeout: float) -> CommandResult:
        if not self._event.wait(timeout):
            raise TimeoutError("renderer command timed out")
        assert self._result is not None
        return self._result


class ControlServer:
    def __init__(
        self,
        config: RendererConfig,
        commands: queue.Queue[ControlCommand],
        readiness: ReadinessSlot,
    ) -> None:
        self._config = config
        self._commands = commands
        self._readiness = readiness
        self._artifacts = ArtifactMaterializer(
            config.artifact_directory, config.maximum_artifact_bytes
        )
        self._server: ThreadingHTTPServer | None = None
        self._thread: threading.Thread | None = None

    def start(self) -> None:
        outer = self

        class Handler(BaseHTTPRequestHandler):
            server_version = "VeoveoSimulationView/1"
            sys_version = ""

            def do_GET(self) -> None:
                if self.path == "/healthz":
                    self._json(HTTPStatus.OK, {"status": "running"})
                    return
                if self.path == "/readyz":
                    value = outer._readiness.get()
                    self._json(
                        HTTPStatus.OK
                        if value.ready
                        else HTTPStatus.SERVICE_UNAVAILABLE,
                        value.response(),
                    )
                    return
                self.send_error(HTTPStatus.NOT_FOUND)

            def do_PUT(self) -> None:
                if match := ARTIFACT_PATH.fullmatch(self.path):
                    self._materialize_artifact(match)
                    return
                self._mutation("PUT")

            def do_DELETE(self) -> None:
                self._mutation("DELETE")

            def _mutation(self, method: str) -> None:
                if not self._authorized():
                    self.send_error(HTTPStatus.UNAUTHORIZED)
                    return
                try:
                    command = self._command(method)
                    outer._commands.put_nowait(command)
                    result = command.wait(120.0)
                    self._json(result.status, result.body)
                except queue.Full:
                    self.send_error(
                        HTTPStatus.SERVICE_UNAVAILABLE,
                        "renderer command queue is full",
                    )
                except TimeoutError as error:
                    self.send_error(HTTPStatus.GATEWAY_TIMEOUT, str(error))
                except (ContractError, ValueError) as error:
                    self.send_error(HTTPStatus.BAD_REQUEST, str(error))

            def _materialize_artifact(self, match: re.Match[str]) -> None:
                if not self._authorized():
                    self.send_error(HTTPStatus.UNAUTHORIZED)
                    return
                try:
                    if self.headers.get("Transfer-Encoding") is not None:
                        raise ContractError(
                            "chunked artifact uploads are unsupported"
                        )
                    raw_length = self.headers.get("Content-Length")
                    if raw_length is None:
                        raise ContractError(
                            "artifact Content-Length is required"
                        )
                    try:
                        length = int(raw_length)
                    except ValueError as error:
                        raise ContractError(
                            "artifact Content-Length is invalid"
                        ) from error
                    content_type = self.headers.get(
                        "Content-Type", ""
                    ).split(";", 1)[0]
                    if content_type.strip().lower() != (
                        "application/octet-stream"
                    ):
                        raise ContractError(
                            "artifact upload must use application/octet-stream"
                        )
                    outer._artifacts.materialize(
                        match.group("digest"),
                        match.group("format"),
                        length,
                        self.rfile,
                    )
                    self._json(HTTPStatus.NO_CONTENT, None)
                except (ContractError, ValueError) as error:
                    self.close_connection = True
                    self.send_error(HTTPStatus.BAD_REQUEST, str(error))

            def _command(self, method: str) -> ControlCommand:
                body = self._body() if method == "PUT" else None
                if method == "DELETE" and self.headers.get(
                    "Content-Length", "0"
                ) != "0":
                    raise ContractError("DELETE requests cannot carry a body")

                if match := SESSION_PATH.fullmatch(self.path):
                    session = match.group("session")
                    if method == "PUT":
                        value = SessionBinding.parse(body)
                        _match_identity(session, value.session_id)
                        return ControlCommand(
                            "put_session", session, None, value
                        )
                    return ControlCommand(
                        "delete_session", session, None, None
                    )
                if match := SCENE_PATH.fullmatch(self.path):
                    if method != "PUT":
                        raise ContractError("scene deletion is unsupported")
                    session = match.group("session")
                    value = SceneBinding.parse(body)
                    _match_identity(session, value.session_id)
                    return ControlCommand("put_scene", session, None, value)
                if match := POSE_PATH.fullmatch(self.path):
                    session = match.group("session")
                    if method == "PUT":
                        value = PoseSourceBinding.parse(body)
                        _match_identity(session, value.session_id)
                        return ControlCommand(
                            "put_pose_source", session, None, value
                        )
                    return ControlCommand(
                        "delete_pose_source", session, None, None
                    )
                if match := CAMERA_PATH.fullmatch(self.path):
                    session = match.group("session")
                    camera = match.group("camera")
                    if method == "PUT":
                        value = CameraBinding.parse(
                            body, outer._config.maximum_render_slots
                        )
                        _match_identity(session, value.session_id)
                        _match_identity(camera, value.camera_id)
                        return ControlCommand(
                            "put_camera", session, camera, value
                        )
                    return ControlCommand(
                        "delete_camera", session, camera, None
                    )
                if match := STREAM_PATH.fullmatch(self.path):
                    session = match.group("session")
                    stream = match.group("stream")
                    if method == "PUT":
                        value = StreamBinding.parse(
                            body, outer._config.maximum_render_slots
                        )
                        _match_identity(session, value.session_id)
                        _match_identity(stream, value.live_view_id)
                        return ControlCommand(
                            "put_stream", session, stream, value
                        )
                    return ControlCommand(
                        "delete_stream", session, stream, None
                    )
                raise ContractError("renderer control path is unsupported")

            def _body(self) -> object:
                if self.headers.get("Transfer-Encoding") is not None:
                    raise ContractError("chunked request bodies are unsupported")
                raw_length = self.headers.get("Content-Length")
                if raw_length is None:
                    raise ContractError("Content-Length is required")
                try:
                    length = int(raw_length)
                except ValueError as error:
                    raise ContractError("Content-Length is invalid") from error
                if length < 2 or length > MAXIMUM_BODY_BYTES:
                    raise ContractError("request body size is invalid")
                content_type = self.headers.get("Content-Type", "").split(
                    ";", 1
                )[0]
                if content_type.strip().lower() != "application/json":
                    raise ContractError("application/json is required")
                try:
                    return json.loads(self.rfile.read(length))
                except (UnicodeDecodeError, json.JSONDecodeError) as error:
                    raise ContractError("request JSON is invalid") from error

            def _authorized(self) -> bool:
                scheme, separator, token = self.headers.get(
                    "Authorization", ""
                ).partition(" ")
                return (
                    separator == " "
                    and scheme.lower() == "bearer"
                    and secrets.compare_digest(
                        token, outer._config.control_token
                    )
                )

            def _json(
                self, status: int, value: dict[str, object] | None
            ) -> None:
                payload = (
                    b""
                    if value is None
                    else json.dumps(
                        value, separators=(",", ":"), sort_keys=True
                    ).encode("utf-8")
                )
                self.send_response(status)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(payload)))
                self.send_header("Cache-Control", "no-store")
                self.end_headers()
                self.wfile.write(payload)

            def log_message(self, format: str, *args: Any) -> None:
                LOGGER.info("%s - %s", self.address_string(), format % args)

        self._server = ThreadingHTTPServer(
            (self._config.control_host, self._config.control_port), Handler
        )
        self._server.daemon_threads = True
        self._thread = threading.Thread(
            target=self._server.serve_forever,
            name="simulation-view-control",
            daemon=True,
        )
        self._thread.start()

    def close(self) -> None:
        if self._server is not None:
            self._server.shutdown()
            self._server.server_close()
        if self._thread is not None:
            self._thread.join(timeout=10.0)


def _match_identity(path_value: str, body_value: str) -> None:
    if path_value != body_value:
        raise ContractError("path and body identities do not match")
