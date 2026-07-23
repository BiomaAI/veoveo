from __future__ import annotations

import os
import uuid
from dataclasses import dataclass
from enum import Enum
from math import sqrt
from pathlib import Path


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


class TileCachePolicy(str, Enum):
    EPHEMERAL = "ephemeral"
    PERSISTENT = "persistent"


@dataclass(frozen=True, slots=True)
class CameraMount:
    translation_xyz_m: tuple[float, float, float]
    orientation_wxyz: tuple[float, float, float, float]

    def __post_init__(self) -> None:
        norm = sqrt(sum(component * component for component in self.orientation_wxyz))
        if abs(norm - 1.0) > 1e-6:
            raise ValueError(
                "UAV_SIM_CAMERA_ORIENTATION_WXYZ must be a unit quaternion"
            )


@dataclass(frozen=True, slots=True)
class CameraConfig:
    width: int
    height: int
    fps: int
    focal_length_mm: float
    clipping_near_m: float
    clipping_far_m: float
    mount: CameraMount

    def __post_init__(self) -> None:
        if self.clipping_near_m >= self.clipping_far_m:
            raise ValueError(
                "UAV_SIM_CAMERA_CLIPPING_NEAR_M must be less than "
                "UAV_SIM_CAMERA_CLIPPING_FAR_M"
            )

    @classmethod
    def from_environment(cls) -> "CameraConfig":
        inverse_sqrt_two = 0.7071067811865476
        mount = CameraMount(
            translation_xyz_m=(
                _float("UAV_SIM_CAMERA_TRANSLATION_X_M", "0.60", -100.0, 100.0),
                _float("UAV_SIM_CAMERA_TRANSLATION_Y_M", "0.0", -100.0, 100.0),
                _float("UAV_SIM_CAMERA_TRANSLATION_Z_M", "0.05", -100.0, 100.0),
            ),
            orientation_wxyz=(
                _float(
                    "UAV_SIM_CAMERA_ORIENTATION_W",
                    str(inverse_sqrt_two),
                    -1.0,
                    1.0,
                ),
                _float("UAV_SIM_CAMERA_ORIENTATION_X", "0.0", -1.0, 1.0),
                _float("UAV_SIM_CAMERA_ORIENTATION_Y", "0.0", -1.0, 1.0),
                _float(
                    "UAV_SIM_CAMERA_ORIENTATION_Z",
                    str(-inverse_sqrt_two),
                    -1.0,
                    1.0,
                ),
            ),
        )
        return cls(
            width=_int("UAV_SIM_CAMERA_WIDTH", "640", 64, 3_840),
            height=_int("UAV_SIM_CAMERA_HEIGHT", "480", 64, 2_160),
            fps=_int("UAV_SIM_CAMERA_FPS", "20", 1, 60),
            focal_length_mm=_float(
                "UAV_SIM_CAMERA_FOCAL_LENGTH_MM", "8.0", 0.1, 1_000.0
            ),
            clipping_near_m=_float(
                "UAV_SIM_CAMERA_CLIPPING_NEAR_M", "0.05", 0.001, 10_000.0
            ),
            clipping_far_m=_float(
                "UAV_SIM_CAMERA_CLIPPING_FAR_M", "100000.0", 0.01, 10_000_000.0
            ),
            mount=mount,
        )


@dataclass(frozen=True, slots=True)
class FollowCameraConfig:
    width: int
    height: int
    fps: int
    focal_length_mm: float
    eye_offset_xyz_m: tuple[float, float, float]
    target_offset_xyz_m: tuple[float, float, float]

    @classmethod
    def from_environment(cls) -> "FollowCameraConfig":
        return cls(
            width=_int("UAV_SIM_FOLLOW_CAMERA_WIDTH", "1280", 64, 3_840),
            height=_int("UAV_SIM_FOLLOW_CAMERA_HEIGHT", "720", 64, 2_160),
            fps=_int("UAV_SIM_FOLLOW_CAMERA_FPS", "20", 1, 60),
            focal_length_mm=_float(
                "UAV_SIM_FOLLOW_CAMERA_FOCAL_LENGTH_MM", "45.0", 0.1, 1_000.0
            ),
            eye_offset_xyz_m=(
                _float(
                    "UAV_SIM_FOLLOW_CAMERA_EYE_OFFSET_X_M",
                    "-2.2",
                    -1_000.0,
                    1_000.0,
                ),
                _float(
                    "UAV_SIM_FOLLOW_CAMERA_EYE_OFFSET_Y_M",
                    "-2.2",
                    -1_000.0,
                    1_000.0,
                ),
                _float(
                    "UAV_SIM_FOLLOW_CAMERA_EYE_OFFSET_Z_M",
                    "1.2",
                    -1_000.0,
                    1_000.0,
                ),
            ),
            target_offset_xyz_m=(
                _float(
                    "UAV_SIM_FOLLOW_CAMERA_TARGET_OFFSET_X_M",
                    "0.0",
                    -1_000.0,
                    1_000.0,
                ),
                _float(
                    "UAV_SIM_FOLLOW_CAMERA_TARGET_OFFSET_Y_M",
                    "0.0",
                    -1_000.0,
                    1_000.0,
                ),
                _float(
                    "UAV_SIM_FOLLOW_CAMERA_TARGET_OFFSET_Z_M",
                    "0.2",
                    -1_000.0,
                    1_000.0,
                ),
            ),
        )


@dataclass(frozen=True, slots=True)
class LiveStreamConfig:
    signal_port: int
    media_port: int
    public_ip: str
    proxy_host: str
    proxy_port: int
    signaling_path: str
    lease_ttl_seconds: int

    def __post_init__(self) -> None:
        if not self.public_ip:
            raise ValueError("UAV_SIM_LIVE_STREAM_PUBLIC_IP must not be empty")
        if not self.proxy_host:
            raise ValueError("UAV_SIM_LIVE_STREAM_PROXY_HOST must not be empty")

    @classmethod
    def from_environment(cls) -> "LiveStreamConfig":
        signaling_path = os.environ.get(
            "UAV_SIM_LIVE_STREAM_SIGNALING_PATH", "/webrtc"
        ).strip()
        if not signaling_path.startswith("/") or ".." in signaling_path:
            raise ValueError(
                "UAV_SIM_LIVE_STREAM_SIGNALING_PATH must be an absolute normalized path"
            )
        return cls(
            signal_port=_int(
                "UAV_SIM_LIVE_STREAM_SIGNAL_PORT", "49100", 1, 65_535
            ),
            media_port=_int(
                "UAV_SIM_LIVE_STREAM_MEDIA_PORT", "47998", 1, 65_535
            ),
            public_ip=os.environ.get(
                "UAV_SIM_LIVE_STREAM_PUBLIC_IP", "127.0.0.1"
            ).strip(),
            proxy_host=os.environ.get(
                "UAV_SIM_LIVE_STREAM_PROXY_HOST", "0.0.0.0"
            ),
            proxy_port=_int(
                "UAV_SIM_LIVE_STREAM_PROXY_PORT", "49101", 1, 65_535
            ),
            signaling_path=signaling_path,
            lease_ttl_seconds=_int(
                "UAV_SIM_LIVE_STREAM_LEASE_TTL_SECONDS", "300", 30, 3_600
            ),
        )


@dataclass(frozen=True, slots=True)
class ScreenshotConfig:
    output_path: Path
    minimum_relative_altitude_m: float
    settle_rendered_frames: int

    @classmethod
    def from_environment(cls) -> "ScreenshotConfig | None":
        raw_output_path = os.environ.get("UAV_SIM_SCREENSHOT_PATH", "").strip()
        if not raw_output_path:
            return None
        output_path = Path(raw_output_path)
        if (
            not output_path.is_absolute()
            or ".." in output_path.parts
            or output_path.suffix.lower() != ".png"
        ):
            raise ValueError(
                "UAV_SIM_SCREENSHOT_PATH must be an absolute normalized PNG path"
            )
        return cls(
            output_path=output_path,
            minimum_relative_altitude_m=_float(
                "UAV_SIM_SCREENSHOT_MINIMUM_RELATIVE_ALTITUDE_M",
                "250.0",
                0.0,
                100_000.0,
            ),
            settle_rendered_frames=_int(
                "UAV_SIM_SCREENSHOT_SETTLE_RENDERED_FRAMES", "30", 1, 600
            ),
        )


@dataclass(frozen=True, slots=True)
class RuntimeConfig:
    session_id: str
    frame_uri: str
    origin_latitude_degrees: float
    origin_longitude_degrees: float
    origin_ellipsoid_height_m: float
    cesium_ion_access_token: str
    cesium_ion_asset_id: int
    tile_cache_policy: TileCachePolicy
    cache_directory: Path
    vehicle_count: int
    adapter_host: str
    adapter_port: int
    physics_hz: int
    rendering_hz: int
    tile_ready_frames: int
    px4_directory: str
    recording_proxy: str
    recording_key: uuid.UUID
    camera: CameraConfig
    follow_camera: FollowCameraConfig
    live_stream: LiveStreamConfig
    screenshot: ScreenshotConfig | None
    extension_directory: str
    exit_after_seconds: float | None

    def __post_init__(self) -> None:
        if self.rendering_hz != self.camera.fps:
            raise ValueError("UAV_SIM_RENDERING_HZ must match UAV_SIM_CAMERA_FPS")
        if self.rendering_hz != self.follow_camera.fps:
            raise ValueError(
                "UAV_SIM_RENDERING_HZ must match UAV_SIM_FOLLOW_CAMERA_FPS"
            )

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
        try:
            cache_policy = TileCachePolicy(_required("UAV_SIM_TILE_CACHE_POLICY"))
        except ValueError as error:
            raise ValueError(
                "UAV_SIM_TILE_CACHE_POLICY must be ephemeral or persistent"
            ) from error

        recording_key = uuid.UUID(_required("UAV_SIM_RECORDING_KEY"))
        cache_directory = Path(
            os.environ.get("XDG_CACHE_HOME", "/var/lib/veoveo/.cache")
        )
        if not cache_directory.is_absolute() or ".." in cache_directory.parts:
            raise ValueError("XDG_CACHE_HOME must be an absolute normalized path")
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
            tile_cache_policy=cache_policy,
            cache_directory=cache_directory,
            vehicle_count=_int("UAV_SIM_VEHICLE_COUNT", "1", 1, 16),
            adapter_host=os.environ.get("UAV_SIM_ADAPTER_HOST", "127.0.0.1"),
            adapter_port=_int("UAV_SIM_ADAPTER_PORT", "8810", 1, 65_535),
            physics_hz=_int("UAV_SIM_PHYSICS_HZ", "250", 30, 1_000),
            rendering_hz=_int("UAV_SIM_RENDERING_HZ", "20", 1, 120),
            tile_ready_frames=_int("UAV_SIM_TILE_READY_FRAMES", "30", 1, 600),
            px4_directory=os.environ.get("UAV_SIM_PX4_DIRECTORY", "/opt/veoveo/px4"),
            recording_proxy=os.environ.get(
                "UAV_SIM_RECORDING_PROXY", "rerun+http://127.0.0.1:9876/proxy"
            ),
            recording_key=recording_key,
            camera=CameraConfig.from_environment(),
            follow_camera=FollowCameraConfig.from_environment(),
            live_stream=LiveStreamConfig.from_environment(),
            screenshot=ScreenshotConfig.from_environment(),
            extension_directory=os.environ.get(
                "UAV_SIM_EXTENSION_DIRECTORY", "/opt/veoveo/extensions"
            ),
            exit_after_seconds=(
                _float("UAV_SIM_EXIT_AFTER_SECONDS", "0", 0.1, 86_400.0)
                if os.environ.get("UAV_SIM_EXIT_AFTER_SECONDS", "").strip()
                else None
            ),
        )
