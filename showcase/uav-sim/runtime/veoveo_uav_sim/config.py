from __future__ import annotations

import os
import uuid
from dataclasses import dataclass


GOOGLE_PHOTOREALISTIC_3D_TILES_ION_ASSET_ID = 2_275_207


def _required(name: str) -> str:
    value = os.environ.get(name, "").strip()
    if not value:
        raise ValueError(f"{name} is required")
    return value


def _float(name: str, default: str, minimum: float, maximum: float) -> float:
    value = float(os.environ.get(name, default))
    if not minimum <= value <= maximum:
        raise ValueError(f"{name} must be between {minimum} and {maximum}")
    return value


def _int(name: str, default: str, minimum: int, maximum: int) -> int:
    value = int(os.environ.get(name, default))
    if not minimum <= value <= maximum:
        raise ValueError(f"{name} must be between {minimum} and {maximum}")
    return value


def _identity(name: str, value: str) -> str:
    if not 1 <= len(value) <= 128 or not all(
        character.isascii()
        and (character.isalnum() or character in {"_", "-", "."})
        for character in value
    ):
        raise ValueError(
            f"{name} must contain 1-128 ASCII letters, digits, underscores, dashes, or dots"
        )
    return value


@dataclass(frozen=True, slots=True)
class RuntimeConfig:
    session_id: str
    frame_uri: str
    origin_latitude_degrees: float
    origin_longitude_degrees: float
    origin_ellipsoid_height_m: float
    cesium_ion_access_token: str
    cesium_ion_asset_id: int
    vehicle_count: int
    adapter_host: str
    adapter_port: int
    physics_hz: int
    rendering_hz: int
    tile_ready_frames: int
    px4_directory: str
    recording_proxy: str
    recording_key: uuid.UUID
    camera_width: int
    camera_height: int
    camera_fps: int
    extension_directory: str
    exit_after_seconds: float | None

    @classmethod
    def from_environment(cls) -> "RuntimeConfig":
        session_id = _identity("UAV_SIM_SESSION_ID", _required("UAV_SIM_SESSION_ID"))
        frame_uri = _required("UAV_SIM_FRAME_URI")
        if not frame_uri.startswith("frames://frame/"):
            raise ValueError("UAV_SIM_FRAME_URI must use frames://frame/{frame_id}")

        world_source = _required("UAV_SIM_WORLD_SOURCE")
        if world_source != "google_photorealistic_3d_tiles":
            raise ValueError(
                "UAV_SIM_WORLD_SOURCE must be google_photorealistic_3d_tiles"
            )
        asset_id = _int(
            "UAV_SIM_CESIUM_ION_ASSET_ID",
            str(GOOGLE_PHOTOREALISTIC_3D_TILES_ION_ASSET_ID),
            1,
            2_147_483_647,
        )
        if asset_id != GOOGLE_PHOTOREALISTIC_3D_TILES_ION_ASSET_ID:
            raise ValueError(
                "UAV_SIM_CESIUM_ION_ASSET_ID must identify Google Photorealistic 3D Tiles"
            )
        if _required("UAV_SIM_TILE_CACHE_POLICY") != "ephemeral":
            raise ValueError("UAV_SIM_TILE_CACHE_POLICY must be ephemeral")

        recording_key = uuid.UUID(_required("UAV_SIM_RECORDING_KEY"))
        return cls(
            session_id=session_id,
            frame_uri=frame_uri,
            origin_latitude_degrees=_float("UAV_SIM_ORIGIN_LATITUDE", "37.7749", -90.0, 90.0),
            origin_longitude_degrees=_float("UAV_SIM_ORIGIN_LONGITUDE", "-122.4194", -180.0, 180.0),
            origin_ellipsoid_height_m=_float(
                "UAV_SIM_ORIGIN_ELLIPSOID_HEIGHT_M", "30.0", -1_000.0, 100_000.0
            ),
            cesium_ion_access_token=_required("CESIUM_ION_ACCESS_TOKEN"),
            cesium_ion_asset_id=asset_id,
            vehicle_count=_int("UAV_SIM_VEHICLE_COUNT", "1", 1, 16),
            adapter_host=os.environ.get("UAV_SIM_ADAPTER_HOST", "127.0.0.1"),
            adapter_port=_int("UAV_SIM_ADAPTER_PORT", "8810", 1, 65_535),
            physics_hz=_int("UAV_SIM_PHYSICS_HZ", "250", 30, 1_000),
            rendering_hz=_int("UAV_SIM_RENDERING_HZ", "30", 1, 120),
            tile_ready_frames=_int("UAV_SIM_TILE_READY_FRAMES", "30", 1, 600),
            px4_directory=os.environ.get("UAV_SIM_PX4_DIRECTORY", "/opt/veoveo/px4"),
            recording_proxy=os.environ.get(
                "UAV_SIM_RECORDING_PROXY", "rerun+http://recording-hub:9876/proxy"
            ),
            recording_key=recording_key,
            camera_width=_int("UAV_SIM_CAMERA_WIDTH", "640", 64, 3_840),
            camera_height=_int("UAV_SIM_CAMERA_HEIGHT", "480", 64, 2_160),
            camera_fps=_int("UAV_SIM_CAMERA_FPS", "20", 1, 60),
            extension_directory=os.environ.get(
                "UAV_SIM_EXTENSION_DIRECTORY", "/opt/veoveo/extensions"
            ),
            exit_after_seconds=(
                _float("UAV_SIM_EXIT_AFTER_SECONDS", "0", 0.1, 86_400.0)
                if os.environ.get("UAV_SIM_EXIT_AFTER_SECONDS", "").strip()
                else None
            ),
        )
