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
class PosePublisherConfig:
    producer_id: str
    producer_spiffe_id: str
    epoch_id: str
    ingress_host: str
    ingress_port: int
    server_hostname: str
    ca_certificate: Path
    client_certificate: Path
    client_private_key: Path
    entity_table_revision: int

    def __post_init__(self) -> None:
        _identity("UAV_SIM_POSE_PRODUCER_ID", self.producer_id)
        _identity("UAV_SIM_POSE_EPOCH_ID", self.epoch_id)
        spiffe_remainder = self.producer_spiffe_id.removeprefix("spiffe://")
        if (
            spiffe_remainder == self.producer_spiffe_id
            or not spiffe_remainder
            or spiffe_remainder.startswith("/")
            or len(self.producer_spiffe_id) > 512
            or any(character.isspace() for character in self.producer_spiffe_id)
        ):
            raise ValueError(
                "UAV_SIM_POSE_PRODUCER_SPIFFE_ID must be a normalized SPIFFE URI"
            )
        for name, value in (
            ("UAV_SIM_POSE_INGRESS_HOST", self.ingress_host),
            ("UAV_SIM_POSE_SERVER_HOSTNAME", self.server_hostname),
        ):
            if not value or "/" in value or any(character.isspace() for character in value):
                raise ValueError(f"{name} must be a DNS name or IP address")
        for name, path in (
            ("UAV_SIM_POSE_CA_CERTIFICATE", self.ca_certificate),
            ("UAV_SIM_POSE_CLIENT_CERTIFICATE", self.client_certificate),
            ("UAV_SIM_POSE_CLIENT_PRIVATE_KEY", self.client_private_key),
        ):
            if not path.is_absolute() or ".." in path.parts:
                raise ValueError(f"{name} must be an absolute normalized path")

    @classmethod
    def from_environment(cls) -> "PosePublisherConfig":
        return cls(
            producer_id=_required("UAV_SIM_POSE_PRODUCER_ID"),
            producer_spiffe_id=_required("UAV_SIM_POSE_PRODUCER_SPIFFE_ID"),
            epoch_id=_required("UAV_SIM_POSE_EPOCH_ID"),
            ingress_host=_required("UAV_SIM_POSE_INGRESS_HOST"),
            ingress_port=_int(
                "UAV_SIM_POSE_INGRESS_PORT", "7443", 1, 65_535
            ),
            server_hostname=_required("UAV_SIM_POSE_SERVER_HOSTNAME"),
            ca_certificate=Path(_required("UAV_SIM_POSE_CA_CERTIFICATE")),
            client_certificate=Path(
                _required("UAV_SIM_POSE_CLIENT_CERTIFICATE")
            ),
            client_private_key=Path(
                _required("UAV_SIM_POSE_CLIENT_PRIVATE_KEY")
            ),
            entity_table_revision=_int(
                "UAV_SIM_POSE_ENTITY_TABLE_REVISION",
                "1",
                1,
                2**63 - 1,
            ),
        )


@dataclass(frozen=True, slots=True)
class RuntimeConfig:
    session_id: str
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
    stream_publication: StreamPublicationConfig | None
    pose_publication: PosePublisherConfig
    extension_directory: str

    def __post_init__(self) -> None:
        if self.rendering_hz != self.camera.fps:
            raise ValueError("UAV_SIM_RENDERING_HZ must match UAV_SIM_CAMERA_FPS")

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
        return cls(
            session_id=session_id,
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
            stream_publication=StreamPublicationConfig.from_environment(),
            pose_publication=PosePublisherConfig.from_environment(),
            extension_directory=os.environ.get(
                "UAV_SIM_EXTENSION_DIRECTORY", "/opt/veoveo/extensions"
            ),
        )
