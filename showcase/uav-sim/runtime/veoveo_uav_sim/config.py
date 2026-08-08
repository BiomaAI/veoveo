from __future__ import annotations

import os
import uuid
from dataclasses import dataclass
from enum import Enum
from math import sqrt
from pathlib import Path

from .operator_camera_config import OperatorLiveViewRuntimeConfig


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


def _bool(name: str, default: str) -> bool:
    value = os.environ.get(name, default)
    if value == "true":
        return True
    if value == "false":
        return False
    raise ValueError(f"{name} must be true or false")


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
class TileStreamingConfig:
    maximum_screen_space_error: float
    maximum_simultaneous_loads: int
    maximum_cached_bytes: int
    preload_ancestors: bool
    preload_siblings: bool
    forbid_holes: bool

    @classmethod
    def from_environment(cls) -> "TileStreamingConfig":
        return cls(
            maximum_screen_space_error=_float(
                "UAV_SIM_TILE_MAXIMUM_SCREEN_SPACE_ERROR", "16.0", 1.0, 64.0
            ),
            maximum_simultaneous_loads=_int(
                "UAV_SIM_TILE_MAXIMUM_SIMULTANEOUS_LOADS", "20", 1, 64
            ),
            maximum_cached_bytes=_int(
                "UAV_SIM_TILE_MAXIMUM_CACHED_BYTES",
                str(2 * 1024 * 1024 * 1024),
                64 * 1024 * 1024,
                16 * 1024 * 1024 * 1024,
            ),
            preload_ancestors=_bool(
                "UAV_SIM_TILE_PRELOAD_ANCESTORS", "true"
            ),
            preload_siblings=_bool(
                "UAV_SIM_TILE_PRELOAD_SIBLINGS", "true"
            ),
            forbid_holes=_bool("UAV_SIM_TILE_FORBID_HOLES", "true"),
        )


class RecordingMapProvider(str, Enum):
    OPEN_STREET_MAP = "openStreetMap"
    MAPBOX_SATELLITE = "mapboxSatellite"


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
    vehicle_id: str
    width: int
    height: int
    fps: int
    bit_rate_bps: int
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
            vehicle_id=_identity(
                "UAV_SIM_CAMERA_VEHICLE_ID",
                os.environ.get("UAV_SIM_CAMERA_VEHICLE_ID", "uav-1"),
            ),
            width=_int("UAV_SIM_CAMERA_WIDTH", "640", 64, 3_840),
            height=_int("UAV_SIM_CAMERA_HEIGHT", "480", 64, 2_160),
            fps=_int("UAV_SIM_CAMERA_FPS", "2", 1, 60),
            bit_rate_bps=_int(
                "UAV_SIM_CAMERA_BIT_RATE_BPS", "750000", 100_000, 50_000_000
            ),
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
class FleetLoopConfig:
    relative_altitude_m: float
    vertical_separation_m: float
    takeoff_timeout_seconds: float
    center_east_m: float
    center_north_m: float
    east_radius_m: float
    north_radius_m: float
    radial_separation_m: float
    waypoint_count: int
    speed_mps: float
    hold_seconds: float

    @classmethod
    def from_environment(cls) -> "FleetLoopConfig":
        return cls(
            relative_altitude_m=_float(
                "UAV_SIM_FLEET_LOOP_RELATIVE_ALTITUDE_M", "450.0", 1.0, 500.0
            ),
            vertical_separation_m=_float(
                "UAV_SIM_FLEET_LOOP_VERTICAL_SEPARATION_M", "15.0", 0.0, 100.0
            ),
            takeoff_timeout_seconds=_float(
                "UAV_SIM_FLEET_LOOP_TAKEOFF_TIMEOUT_SECONDS",
                "420.0",
                30.0,
                1_800.0,
            ),
            center_east_m=_float(
                "UAV_SIM_FLEET_LOOP_CENTER_EAST_M", "1700.0", -100_000.0, 100_000.0
            ),
            center_north_m=_float(
                "UAV_SIM_FLEET_LOOP_CENTER_NORTH_M", "3000.0", -100_000.0, 100_000.0
            ),
            east_radius_m=_float(
                "UAV_SIM_FLEET_LOOP_EAST_RADIUS_M", "2500.0", 10.0, 100_000.0
            ),
            north_radius_m=_float(
                "UAV_SIM_FLEET_LOOP_NORTH_RADIUS_M", "9000.0", 10.0, 100_000.0
            ),
            radial_separation_m=_float(
                "UAV_SIM_FLEET_LOOP_RADIAL_SEPARATION_M", "100.0", 0.0, 1_000.0
            ),
            waypoint_count=_int(
                "UAV_SIM_FLEET_LOOP_WAYPOINT_COUNT", "32", 4, 256
            ),
            speed_mps=_float(
                "UAV_SIM_FLEET_LOOP_SPEED_MPS", "25.0", 0.1, 100.0
            ),
            hold_seconds=_float(
                "UAV_SIM_FLEET_LOOP_HOLD_SECONDS", "0.0", 0.0, 3_600.0
            ),
        )


@dataclass(frozen=True, slots=True)
class RecordingConfig:
    telemetry_hz: int
    queue_capacity: int
    map_provider: RecordingMapProvider

    @classmethod
    def from_environment(cls) -> "RecordingConfig":
        try:
            map_provider = RecordingMapProvider(
                os.environ.get(
                    "UAV_SIM_RECORDING_MAP_PROVIDER",
                    RecordingMapProvider.OPEN_STREET_MAP.value,
                )
            )
        except ValueError as error:
            raise ValueError(
                "UAV_SIM_RECORDING_MAP_PROVIDER must be openStreetMap or mapboxSatellite"
            ) from error
        return cls(
            telemetry_hz=_int(
                "UAV_SIM_RECORDING_TELEMETRY_HZ", "5", 1, 120
            ),
            queue_capacity=_int(
                "UAV_SIM_RECORDING_QUEUE_CAPACITY", "256", 16, 65_536
            ),
            map_provider=map_provider,
        )


@dataclass(frozen=True, slots=True)
class StreamPublicationConfig:
    host: str
    port: int
    payload_type: int
    source_vehicle_id: str

    def __post_init__(self) -> None:
        if (
            not self.host
            or "/" in self.host
            or any(character.isspace() for character in self.host)
        ):
            raise ValueError("UAV_SIM_STREAM_HOST must be a DNS name or IP address")
        if not 96 <= self.payload_type <= 127:
            raise ValueError(
                "UAV_SIM_STREAM_PAYLOAD_TYPE must be a dynamic RTP payload type"
            )
        _identity("UAV_SIM_STREAM_SOURCE_VEHICLE_ID", self.source_vehicle_id)

    @classmethod
    def from_environment(cls) -> "StreamPublicationConfig | None":
        host = os.environ.get("UAV_SIM_STREAM_HOST", "").strip()
        if not host:
            for name in (
                "UAV_SIM_STREAM_PORT",
                "UAV_SIM_STREAM_PAYLOAD_TYPE",
                "UAV_SIM_STREAM_SOURCE_VEHICLE_ID",
            ):
                if os.environ.get(name, "").strip():
                    raise ValueError(
                        f"{name} requires UAV_SIM_STREAM_HOST"
                    )
            return None
        return cls(
            host=host,
            port=_int("UAV_SIM_STREAM_PORT", "9000", 1, 65_535),
            payload_type=_int(
                "UAV_SIM_STREAM_PAYLOAD_TYPE", "96", 96, 127
            ),
            source_vehicle_id=_identity(
                "UAV_SIM_STREAM_SOURCE_VEHICLE_ID",
                os.environ.get(
                    "UAV_SIM_STREAM_SOURCE_VEHICLE_ID", "uav-1"
                ),
            ),
        )


@dataclass(frozen=True, slots=True)
class RuntimeConfig:
    session_id: str
    cesium_ion_access_token: str
    cesium_ion_asset_id: int
    tile_cache_policy: TileCachePolicy
    tile_streaming: TileStreamingConfig
    cache_directory: Path
    vehicle_count: int
    adapter_host: str
    adapter_port: int
    runtime_event_socket: Path
    physics_hz: int
    rendering_hz: int
    tile_ready_frames: int
    px4_connect_timeout_seconds: float
    px4_directory: str
    recording_proxy: str
    recording_key: uuid.UUID
    recording: RecordingConfig
    camera: CameraConfig
    operator_live_view: OperatorLiveViewRuntimeConfig
    fleet_loop: FleetLoopConfig
    stream_publication: StreamPublicationConfig | None
    extension_directory: str

    def __post_init__(self) -> None:
        maximum_operator_fps = max(
            camera.optics.frame_rate_hz
            for camera in self.operator_live_view.cameras
            if camera.stream_policy.value != "disabled"
        )
        if self.rendering_hz < maximum_operator_fps:
            raise ValueError(
                "UAV_SIM_RENDERING_HZ must be at least the maximum active "
                "operator-camera frame rate"
            )
        if self.recording.telemetry_hz > self.physics_hz:
            raise ValueError(
                "UAV_SIM_RECORDING_TELEMETRY_HZ must not exceed UAV_SIM_PHYSICS_HZ"
            )
        admitted_vehicle_ids = {
            f"uav-{index + 1}" for index in range(self.vehicle_count)
        }
        if self.camera.vehicle_id not in admitted_vehicle_ids:
            raise ValueError(
                "UAV_SIM_CAMERA_VEHICLE_ID must identify an admitted fleet vehicle"
            )
        if (
            self.stream_publication is not None
            and self.stream_publication.source_vehicle_id != self.camera.vehicle_id
        ):
            raise ValueError(
                "UAV_SIM_STREAM_SOURCE_VEHICLE_ID must match UAV_SIM_CAMERA_VEHICLE_ID"
            )
        highest_loop_altitude = self.fleet_loop.relative_altitude_m + (
            self.vehicle_count - 1
        ) * self.fleet_loop.vertical_separation_m
        if highest_loop_altitude > 500.0:
            raise ValueError(
                "fleet loop altitude plus vehicle separation must not exceed 500 metres"
            )

    @classmethod
    def from_environment(cls) -> "RuntimeConfig":
        session_id = _identity("UAV_SIM_SESSION_ID", _required("UAV_SIM_SESSION_ID"))

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
        runtime_event_socket = Path(
            os.environ.get(
                "UAV_SIM_RUNTIME_EVENT_SOCKET",
                "/var/run/veoveo-uav-sim/runtime-events.sock",
            )
        )
        if (
            not runtime_event_socket.is_absolute()
            or ".." in runtime_event_socket.parts
            or runtime_event_socket.name != "runtime-events.sock"
        ):
            raise ValueError(
                "UAV_SIM_RUNTIME_EVENT_SOCKET must be an absolute normalized runtime-events.sock path"
            )
        return cls(
            session_id=session_id,
            cesium_ion_access_token=_required("CESIUM_ION_ACCESS_TOKEN"),
            cesium_ion_asset_id=asset_id,
            tile_cache_policy=cache_policy,
            tile_streaming=TileStreamingConfig.from_environment(),
            cache_directory=cache_directory,
            vehicle_count=_int("UAV_SIM_VEHICLE_COUNT", "1", 1, 16),
            adapter_host=os.environ.get("UAV_SIM_ADAPTER_HOST", "127.0.0.1"),
            adapter_port=_int("UAV_SIM_ADAPTER_PORT", "8810", 1, 65_535),
            runtime_event_socket=runtime_event_socket,
            physics_hz=_int("UAV_SIM_PHYSICS_HZ", "60", 30, 1_000),
            rendering_hz=_int("UAV_SIM_RENDERING_HZ", "2", 1, 120),
            tile_ready_frames=_int("UAV_SIM_TILE_READY_FRAMES", "30", 1, 600),
            px4_connect_timeout_seconds=_float(
                "UAV_SIM_PX4_CONNECT_TIMEOUT_SECONDS", "180.0", 30.0, 600.0
            ),
            px4_directory=os.environ.get("UAV_SIM_PX4_DIRECTORY", "/opt/veoveo/px4"),
            recording_proxy=os.environ.get(
                "UAV_SIM_RECORDING_PROXY", "rerun+http://127.0.0.1:9876/proxy"
            ),
            recording_key=recording_key,
            recording=RecordingConfig.from_environment(),
            camera=CameraConfig.from_environment(),
            operator_live_view=OperatorLiveViewRuntimeConfig.from_json(
                _required("UAV_SIM_OPERATOR_CAMERAS_JSON"),
                viewer_slot_count=_int(
                    "UAV_SIM_LIVE_VIEWER_SLOTS", "2", 1, 32
                ),
                activation_timeout_seconds=_float(
                    "UAV_SIM_LIVE_ACTIVATION_TIMEOUT_SECONDS",
                    "10.0",
                    0.1,
                    60.0,
                ),
                signaling_port_base=_int(
                    "UAV_SIM_LIVE_SIGNALING_PORT_BASE", "49100", 1, 65_535
                ),
                media_port_base=_int(
                    "UAV_SIM_LIVE_MEDIA_PORT_BASE", "47998", 1, 65_535
                ),
                public_media_ip=_required("UAV_SIM_LIVE_PUBLIC_MEDIA_IP"),
            ),
            fleet_loop=FleetLoopConfig.from_environment(),
            stream_publication=StreamPublicationConfig.from_environment(),
            extension_directory=os.environ.get(
                "UAV_SIM_EXTENSION_DIRECTORY", "/opt/veoveo/extensions"
            ),
        )
