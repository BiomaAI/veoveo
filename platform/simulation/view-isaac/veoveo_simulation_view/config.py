from __future__ import annotations

import os
from dataclasses import dataclass
from pathlib import Path
from urllib.parse import urlsplit

from .layers import LayerCatalog


def _required(name: str) -> str:
    value = os.environ.get(name, "")
    if not value or any(character.isspace() for character in value):
        raise ValueError(f"{name} must contain non-whitespace content")
    return value


def _integer(name: str, default: int, minimum: int, maximum: int) -> int:
    raw = os.environ.get(name, str(default))
    try:
        value = int(raw)
    except ValueError as error:
        raise ValueError(f"{name} must be an integer") from error
    if not minimum <= value <= maximum:
        raise ValueError(f"{name} must be between {minimum} and {maximum}")
    return value


def _absolute_directory(name: str, default: str) -> Path:
    path = Path(os.environ.get(name, default))
    if not path.is_absolute() or path == Path("/") or len(path.parts) < 3:
        raise ValueError(f"{name} must be a narrow absolute path")
    return path


def _internal_http_url(name: str) -> str:
    value = _required(name)
    parsed = urlsplit(value)
    if (
        parsed.scheme != "http"
        or not parsed.hostname
        or parsed.username is not None
        or parsed.password is not None
        or parsed.query
        or parsed.fragment
        or not parsed.path.endswith("/runtime-events/renderer")
    ):
        raise ValueError(f"{name} must be a credential-free internal HTTP renderer event URL")
    return value.rstrip("/")


@dataclass(frozen=True, slots=True)
class RendererConfig:
    control_host: str
    control_port: int
    control_token: str
    runtime_event_url: str
    artifact_directory: Path
    maximum_artifact_bytes: int
    pose_directory: Path
    cache_directory: Path
    maximum_render_slots: int
    signaling_port_base: int
    media_port_base: int
    public_media_ip: str
    stream_target_fps: int
    probe_width: int
    probe_height: int
    probe_fps: int
    frame_stale_after_ms: int
    layer_catalog: LayerCatalog

    @classmethod
    def from_environment(cls) -> "RendererConfig":
        token = _required("SIMULATION_VIEW_RENDERER_CONTROL_TOKEN")
        if not 32 <= len(token) <= 512:
            raise ValueError(
                "SIMULATION_VIEW_RENDERER_CONTROL_TOKEN must contain 32 to 512 characters"
            )
        slots = _integer("SIMULATION_VIEW_MAXIMUM_RENDER_SLOTS", 4, 1, 32)
        signaling_base = _integer(
            "SIMULATION_VIEW_SIGNALING_PORT_BASE", 49100, 1, 65535
        )
        media_base = _integer(
            "SIMULATION_VIEW_MEDIA_PORT_BASE", 47998, 1, 65535
        )
        if signaling_base + slots - 1 > 65535:
            raise ValueError("signaling port range exceeds 65535")
        if media_base + slots - 1 > 65535:
            raise ValueError("media port range exceeds 65535")
        return cls(
            control_host=os.environ.get(
                "SIMULATION_VIEW_RENDERER_CONTROL_HOST", "0.0.0.0"
            ),
            control_port=_integer(
                "SIMULATION_VIEW_RENDERER_CONTROL_PORT", 8810, 1, 65535
            ),
            control_token=token,
            runtime_event_url=_internal_http_url(
                "SIMULATION_VIEW_RUNTIME_EVENT_URL"
            ),
            artifact_directory=_absolute_directory(
                "SIMULATION_VIEW_ARTIFACT_DIRECTORY",
                "/var/lib/veoveo/simulation-view/artifacts",
            ),
            maximum_artifact_bytes=_integer(
                "SIMULATION_VIEW_MAXIMUM_ARTIFACT_BYTES",
                4 * 1024 * 1024 * 1024,
                1,
                16 * 1024 * 1024 * 1024,
            ),
            pose_directory=_absolute_directory(
                "SIMULATION_VIEW_POSE_DIRECTORY",
                "/dev/shm/veoveo/simulation-view",
            ),
            cache_directory=_absolute_directory(
                "SIMULATION_VIEW_RENDERER_CACHE_DIRECTORY",
                "/var/lib/veoveo/runtime-cache/simulation-view",
            ),
            maximum_render_slots=slots,
            signaling_port_base=signaling_base,
            media_port_base=media_base,
            public_media_ip=_required("SIMULATION_VIEW_PUBLIC_MEDIA_IP"),
            stream_target_fps=_integer(
                "SIMULATION_VIEW_STREAM_TARGET_FPS", 30, 1, 120
            ),
            probe_width=_integer(
                "SIMULATION_VIEW_PROBE_WIDTH", 640, 64, 4096
            ),
            probe_height=_integer(
                "SIMULATION_VIEW_PROBE_HEIGHT", 360, 64, 4096
            ),
            probe_fps=_integer("SIMULATION_VIEW_PROBE_FPS", 20, 1, 120),
            frame_stale_after_ms=_integer(
                "SIMULATION_VIEW_FRAME_STALE_AFTER_MS", 500, 1, 60_000
            ),
            layer_catalog=LayerCatalog.load(
                Path(
                    os.environ.get(
                        "SIMULATION_VIEW_LAYER_CATALOG",
                        "/etc/veoveo/simulation-view/layers.json",
                    )
                )
            ),
        )

    def prepare_directories(self) -> None:
        if (
            not self.artifact_directory.is_dir()
            or self.artifact_directory.is_symlink()
        ):
            raise ValueError(
                "SIMULATION_VIEW_ARTIFACT_DIRECTORY must be a materialized directory"
            )
        for directory in (self.pose_directory, self.cache_directory):
            directory.mkdir(parents=True, exist_ok=True)
            directory.chmod(0o700)
