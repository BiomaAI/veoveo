from __future__ import annotations

import json
from typing import Iterable

import av
import numpy as np
import rerun as rr

from .config import RuntimeConfig
from .camera_quality import CameraFrameQuality, normalize_rgb_frame
from .state import VehicleTelemetry
from .stream_output import RtpH264Publisher
from .world_config import WorldConfiguration


class H264CameraStream:
    # TODO(GPU): Replace the existing RTX capture readback with a direct CUDA
    # render-product handoff. Encoding already fails closed on NVIDIA NVENC,
    # and its one Annex B packet stream fans out unchanged to Stream and Rerun.
    def __init__(
        self,
        recording: rr.RecordingStream,
        entity_path: str,
        width: int,
        height: int,
        fps: int,
        stream_output: RtpH264Publisher | None,
    ) -> None:
        self._recording = recording
        self._entity_path = entity_path
        self._width = width
        self._height = height
        self._stream_output = stream_output
        self._container = av.open("/dev/null", "w", format="h264")
        self._stream = self._container.add_stream("h264_nvenc", rate=fps)
        self._stream.width = width
        self._stream.height = height
        self._stream.pix_fmt = "yuv420p"
        self._stream.max_b_frames = 0
        self._stream.codec_context.gop_size = fps
        self._stream.options = {
            "profile": "baseline",
            "preset": "p1",
            "tune": "ull",
            "zerolatency": "1",
            "forced-idr": "1",
        }
        if self._stream.codec_context.codec.name != "h264_nvenc":
            raise RuntimeError("UAV camera H.264 encoder is not NVIDIA NVENC")
        self._recording.log(
            entity_path,
            rr.VideoStream(codec=rr.VideoCodec.H264),
            rr.Pinhole(resolution=[width, height], focal_length=width / 2.0),
            static=True,
        )

    def encode(self, rgb: np.ndarray, simulation_time_s: float, physics_step: int) -> None:
        frame = av.VideoFrame.from_ndarray(normalize_rgb_frame(rgb), format="rgb24")
        for packet in self._stream.encode(frame):
            sample = bytes(packet)
            if self._stream_output is not None:
                self._stream_output.publish(sample, simulation_time_s)
            self._set_time(simulation_time_s, physics_step)
            if packet.is_keyframe:
                self._recording.log(
                    self._entity_path, _video_packet(sample, is_keyframe=True)
                )
                self._recording.log(
                    self._entity_path,
                    rr.Pinhole(
                        resolution=[self._width, self._height],
                        focal_length=self._width / 2.0,
                    ),
                )
            else:
                self._recording.log(self._entity_path, _video_packet(sample))

    def close(self, simulation_time_s: float, physics_step: int) -> None:
        for packet in self._stream.encode(None):
            sample = bytes(packet)
            if self._stream_output is not None:
                self._stream_output.publish(sample, simulation_time_s)
            self._set_time(simulation_time_s, physics_step)
            self._recording.log(
                self._entity_path,
                _video_packet(sample, is_keyframe=packet.is_keyframe),
            )
        self._container.close()
        if self._stream_output is not None:
            self._stream_output.close()

    def _set_time(self, simulation_time_s: float, physics_step: int) -> None:
        self._recording.set_time("simulation_time", duration=simulation_time_s)
        self._recording.set_time("physics_step", sequence=physics_step)


def _video_packet(
    sample: bytes, *, is_keyframe: bool = False
) -> rr.VideoStream:
    fields: dict[str, object] = {
        "codec": rr.VideoCodec.H264,
        "sample": sample,
    }
    if is_keyframe:
        fields["is_keyframe"] = True
    return rr.VideoStream.from_fields(**fields)


class RecordingPublisher:
    def __init__(self, config: RuntimeConfig, world: WorldConfiguration) -> None:
        self._config = config
        self._root = f"/world/uav-sim/{config.session_id}"
        self._recording = rr.RecordingStream(
            "veoveo-uav-sim", recording_id=config.recording_key
        )
        self._recording.connect_grpc(config.recording_proxy)
        self._cameras: dict[str, H264CameraStream] = {}
        self._recording.log(
            self._root,
            rr.AnyValues(
                world_revision_uri=world.revision_uri,
                world_spec_sha256=world.spec_sha256,
                simulation_frame_uri=world.simulation_frame_uri,
                origin_latitude_degrees=world.georeference_origin.latitude_degrees,
                origin_longitude_degrees=world.georeference_origin.longitude_degrees,
                origin_ellipsoid_height_m=(
                    world.georeference_origin.ellipsoid_height_m
                ),
            ),
            static=True,
        )

    @property
    def recording_key(self) -> str:
        return str(self._config.recording_key)

    def add_camera(self, vehicle_id: str) -> H264CameraStream:
        entity_path = f"{self._root}/vehicle/{vehicle_id}/camera/down"
        stream_output = (
            RtpH264Publisher(self._config.stream_publication)
            if self._config.stream_publication is not None
            and vehicle_id
            == self._config.stream_publication.source_vehicle_id
            else None
        )
        camera = H264CameraStream(
            self._recording,
            entity_path,
            self._config.camera.width,
            self._config.camera.height,
            self._config.camera.fps,
            stream_output,
        )
        self._cameras[vehicle_id] = camera
        return camera

    def camera(self, vehicle_id: str) -> H264CameraStream:
        return self._cameras[vehicle_id]

    def log_frame(
        self,
        telemetry: Iterable[VehicleTelemetry],
        simulation_time_s: float,
        physics_step: int,
    ) -> None:
        self._set_time(simulation_time_s, physics_step)
        for vehicle in telemetry:
            base = f"{self._root}/vehicle/{vehicle.vehicle_id}"
            self._recording.log(
                base,
                rr.Transform3D(
                    translation=vehicle.position_enu,
                    quaternion=rr.Quaternion(xyzw=vehicle.attitude_xyzw),
                ),
            )
            self._recording.log(
                f"{base}/velocity_enu_mps",
                rr.Arrows3D(vectors=[vehicle.linear_velocity_enu_mps]),
            )
            self._recording.log(
                f"{base}/battery_percent", rr.Scalars([vehicle.battery_percent])
            )
            self._recording.log(
                f"{base}/collision_count", rr.Scalars([vehicle.collision_count])
            )
            self._recording.log(
                f"{base}/flight_state", rr.TextLog(vehicle.flight_state)
            )

    def log_imu(
        self,
        vehicle_id: str,
        linear_acceleration: tuple[float, float, float],
        angular_velocity: tuple[float, float, float],
        simulation_time_s: float,
        physics_step: int,
    ) -> None:
        self._set_time(simulation_time_s, physics_step)
        base = f"{self._root}/vehicle/{vehicle_id}/imu"
        self._recording.log(
            f"{base}/linear_acceleration_mps2", rr.Arrows3D(vectors=[linear_acceleration])
        )
        self._recording.log(
            f"{base}/angular_velocity_rps", rr.Arrows3D(vectors=[angular_velocity])
        )

    def log_tiles(
        self,
        resident_tiles: int,
        loading_tiles: int,
        lifecycle: str,
        simulation_time_s: float,
        physics_step: int,
    ) -> None:
        self._set_time(simulation_time_s, physics_step)
        base = f"{self._root}/tiles"
        self._recording.log(f"{base}/resident", rr.Scalars([resident_tiles]))
        self._recording.log(f"{base}/loading", rr.Scalars([loading_tiles]))
        self._recording.log(f"{base}/lifecycle", rr.TextLog(lifecycle))

    def log_camera_quality(
        self,
        vehicle_id: str,
        quality: CameraFrameQuality,
        lifecycle: str,
        simulation_time_s: float,
        physics_step: int,
    ) -> None:
        self._set_time(simulation_time_s, physics_step)
        base = f"{self._root}/vehicle/{vehicle_id}/camera/down/quality"
        self._recording.log(f"{base}/mean_luma", rr.Scalars([quality.mean_luma]))
        self._recording.log(
            f"{base}/dynamic_range", rr.Scalars([quality.dynamic_range])
        )
        self._recording.log(
            f"{base}/non_black_fraction",
            rr.Scalars([quality.non_black_fraction]),
        )
        self._recording.log(f"{base}/lifecycle", rr.TextLog(lifecycle))

    def log_mission(self, mission_id: str, lifecycle: str, detail: dict[str, object]) -> None:
        self._recording.log(
            f"{self._root}/mission/{mission_id}",
            rr.TextLog(json.dumps({"lifecycle": lifecycle, **detail}, sort_keys=True)),
        )

    def close(self, simulation_time_s: float, physics_step: int) -> None:
        for camera in self._cameras.values():
            camera.close(simulation_time_s, physics_step)
        self._recording.flush()
        # Closing the network sink is the producer's explicit capture boundary.
        # Recording Hub freezes the final segment after its short idle grace
        # period and records the timestamp of the last received message.
        self._recording.disconnect()

    def _set_time(self, simulation_time_s: float, physics_step: int) -> None:
        self._recording.set_time("simulation_time", duration=simulation_time_s)
        self._recording.set_time("physics_step", sequence=physics_step)
