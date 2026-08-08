from __future__ import annotations

import json
import logging
import queue
import threading
from dataclasses import dataclass

import rerun as rr
import rerun.blueprint as rrb

from .camera_quality import CameraFrameQuality
from .config import RecordingMapProvider, RuntimeConfig
from .event_queue import NonBlockingEventQueue
from .geo import enu_to_geodetic
from .h264 import NativeH264AccessUnit
from .state import VehicleTelemetry
from .stream_output import RtpH264Publisher
from .world_config import WorldConfiguration


LOGGER = logging.getLogger("veoveo.uav_sim.recording")


@dataclass(frozen=True, slots=True)
class ImuTelemetry:
    vehicle_id: str
    linear_acceleration_mps2: tuple[float, float, float]
    angular_velocity_rps: tuple[float, float, float]


@dataclass(frozen=True, slots=True)
class RecordingPublisherStatus:
    lifecycle: str
    queued_events: int
    dropped_events: int
    last_error: str | None


@dataclass(frozen=True, slots=True)
class _FrameEvent:
    vehicles: tuple[VehicleTelemetry, ...]
    imu: tuple[ImuTelemetry, ...]
    simulation_time_s: float
    physics_step: int


@dataclass(frozen=True, slots=True)
class _CameraEvent:
    access_unit: NativeH264AccessUnit
    simulation_time_s: float
    physics_step: int


@dataclass(frozen=True, slots=True)
class _CameraQualityEvent:
    quality: CameraFrameQuality
    lifecycle: str
    simulation_time_s: float
    physics_step: int


@dataclass(frozen=True, slots=True)
class _TilesEvent:
    resident_tiles: int
    visible_tiles: int
    loading_tiles: int
    recovery_count: int
    lifecycle: str
    simulation_time_s: float
    physics_step: int


@dataclass(frozen=True, slots=True)
class _MissionEvent:
    mission_id: str
    lifecycle: str
    detail_json: str


@dataclass(frozen=True, slots=True)
class _StopEvent:
    pass


type _RecordingEvent = (
    _FrameEvent
    | _CameraEvent
    | _CameraQualityEvent
    | _TilesEvent
    | _MissionEvent
    | _StopEvent
)


class RecordedH264CameraStream:
    """Fan one native Isaac NVENC access unit to Recording and live RTP."""

    def __init__(
        self,
        recording: rr.RecordingStream,
        entity_path: str,
        width: int,
        height: int,
        stream_output: RtpH264Publisher | None,
    ) -> None:
        self._recording = recording
        self._entity_path = entity_path
        self._stream_output = stream_output
        self._recording.log(
            entity_path,
            rr.VideoStream(codec=rr.VideoCodec.H264),
            rr.Pinhole(resolution=[width, height], focal_length=width / 2.0),
            static=True,
        )

    def publish(
        self,
        access_unit: NativeH264AccessUnit,
        simulation_time_s: float,
        physics_step: int,
    ) -> None:
        if not access_unit.is_keyframe:
            raise RuntimeError("native Isaac camera access unit is not an IDR")
        if self._stream_output is not None:
            self._stream_output.publish(access_unit.sample, simulation_time_s)
        self._set_time(simulation_time_s, physics_step)
        self._recording.log(
            self._entity_path,
            _video_packet(access_unit.sample, is_keyframe=True),
        )

    def close(self) -> None:
        if self._stream_output is not None:
            self._stream_output.close()

    def _set_time(self, simulation_time_s: float, physics_step: int) -> None:
        self._recording.set_time("simulation_time", duration=simulation_time_s)
        self._recording.set_time("physics_step", sequence=physics_step)


def _video_packet(sample: bytes, *, is_keyframe: bool = False) -> rr.VideoStream:
    fields: dict[str, object] = {
        "sample": sample,
    }
    if is_keyframe:
        fields["is_keyframe"] = True
    return rr.VideoStream.from_fields(**fields)


class RecordingPublisher:
    """Nonblocking simulation-side facade over one retrying recording worker."""

    def __init__(self, config: RuntimeConfig, world: WorldConfiguration) -> None:
        self._config = config
        self._world = world
        self._events = NonBlockingEventQueue[_RecordingEvent](
            config.recording.queue_capacity
        )
        self._status_lock = threading.Lock()
        self._lifecycle = "connecting"
        self._last_error: str | None = None
        self._closed = threading.Event()
        self._worker = threading.Thread(
            target=self._run,
            name="uav-recording",
            daemon=True,
        )
        self._worker.start()

    @property
    def recording_key(self) -> str:
        return str(self._config.recording_key)

    def offer_frame(
        self,
        telemetry: list[VehicleTelemetry],
        imu: list[ImuTelemetry],
        simulation_time_s: float,
        physics_step: int,
    ) -> None:
        self._events.offer(
            _FrameEvent(
                vehicles=tuple(telemetry),
                imu=tuple(imu),
                simulation_time_s=simulation_time_s,
                physics_step=physics_step,
            )
        )

    def offer_camera_access_unit(
        self,
        access_unit: NativeH264AccessUnit,
        simulation_time_s: float,
        physics_step: int,
    ) -> None:
        self._events.offer(
            _CameraEvent(
                access_unit=access_unit,
                simulation_time_s=simulation_time_s,
                physics_step=physics_step,
            )
        )

    def log_camera_quality(
        self,
        quality: CameraFrameQuality,
        lifecycle: str,
        simulation_time_s: float,
        physics_step: int,
    ) -> None:
        self._events.offer(
            _CameraQualityEvent(
                quality=quality,
                lifecycle=lifecycle,
                simulation_time_s=simulation_time_s,
                physics_step=physics_step,
            )
        )

    def log_tiles(
        self,
        resident_tiles: int,
        visible_tiles: int,
        loading_tiles: int,
        recovery_count: int,
        lifecycle: str,
        simulation_time_s: float,
        physics_step: int,
    ) -> None:
        self._events.offer(
            _TilesEvent(
                resident_tiles=resident_tiles,
                visible_tiles=visible_tiles,
                loading_tiles=loading_tiles,
                recovery_count=recovery_count,
                lifecycle=lifecycle,
                simulation_time_s=simulation_time_s,
                physics_step=physics_step,
            )
        )

    def log_mission(
        self, mission_id: str, lifecycle: str, detail: dict[str, object]
    ) -> None:
        self._events.offer(
            _MissionEvent(
                mission_id=mission_id,
                lifecycle=lifecycle,
                detail_json=json.dumps(detail, sort_keys=True),
            )
        )

    def status(self) -> RecordingPublisherStatus:
        with self._status_lock:
            return RecordingPublisherStatus(
                lifecycle=self._lifecycle,
                queued_events=self._events.depth(),
                dropped_events=self._events.dropped(),
                last_error=self._last_error,
            )

    def close(self) -> None:
        if self._closed.is_set():
            return
        self._closed.set()
        self._events.offer(_StopEvent())
        self._worker.join(timeout=30.0)
        if self._worker.is_alive():
            LOGGER.error("recording worker did not stop within 30 seconds")

    def _run(self) -> None:
        while not self._closed.is_set():
            sink: _RecordingSink | None = None
            try:
                sink = _RecordingSink(self._config, self._world)
                self._set_status("ready", None)
                while True:
                    try:
                        event = self._events.take(0.5)
                    except queue.Empty:
                        if self._closed.is_set():
                            return
                        continue
                    if isinstance(event, _StopEvent):
                        sink.close()
                        self._set_status("stopped", None)
                        return
                    sink.handle(event)
            except Exception as error:
                message = _bounded_diagnostic(error)
                LOGGER.exception(
                    "governed recording worker failed; simulation continues"
                )
                self._set_status("degraded", message)
                if sink is not None:
                    sink.abort()
                if self._closed.wait(2.0):
                    return

    def _set_status(self, lifecycle: str, error: str | None) -> None:
        with self._status_lock:
            self._lifecycle = lifecycle
            self._last_error = error


class _RecordingSink:
    def __init__(self, config: RuntimeConfig, world: WorldConfiguration) -> None:
        self._config = config
        self._world = world
        self._root = f"/world/uav-sim/{config.session_id}"
        self._recording = rr.RecordingStream(
            "veoveo-uav-sim",
            recording_id=config.recording_key,
            batcher_config=rr.ChunkBatcherConfig.LOW_LATENCY(),
        )
        self._recording.connect_grpc(config.recording_proxy)
        rr.send_blueprint(
            _recording_blueprint(config, self._root),
            make_active=True,
            make_default=True,
            recording=self._recording,
        )
        self._last_vehicle_status: dict[str, tuple[object, ...]] = {}
        self._last_tiles: tuple[int, int, int, int, str] | None = None
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
        camera_entity = f"{self._root}/vehicle/{config.camera.vehicle_id}/camera/down"
        stream_output = (
            RtpH264Publisher(config.stream_publication)
            if config.stream_publication is not None
            else None
        )
        self._camera = RecordedH264CameraStream(
            self._recording,
            camera_entity,
            config.camera.width,
            config.camera.height,
            stream_output,
        )

    def handle(
        self,
        event: _FrameEvent
        | _CameraEvent
        | _CameraQualityEvent
        | _TilesEvent
        | _MissionEvent,
    ) -> None:
        if isinstance(event, _FrameEvent):
            self._log_frame(event)
        elif isinstance(event, _CameraEvent):
            self._camera.publish(
                event.access_unit, event.simulation_time_s, event.physics_step
            )
        elif isinstance(event, _CameraQualityEvent):
            self._log_camera_quality(event)
        elif isinstance(event, _TilesEvent):
            self._log_tiles(event)
        else:
            self._recording.log(
                f"{self._root}/mission/{event.mission_id}",
                rr.TextLog(
                    json.dumps(
                        {
                            "lifecycle": event.lifecycle,
                            **json.loads(event.detail_json),
                        },
                        sort_keys=True,
                    )
                ),
            )

    def close(self) -> None:
        self._camera.close()
        self._recording.flush()
        self._recording.disconnect()

    def abort(self) -> None:
        try:
            self._recording.disconnect()
        except Exception:
            LOGGER.exception("recording sink disconnect failed")

    def _log_frame(self, event: _FrameEvent) -> None:
        self._set_time(event.simulation_time_s, event.physics_step)
        positions = [vehicle.position_enu for vehicle in event.vehicles]
        velocities = [vehicle.linear_velocity_enu_mps for vehicle in event.vehicles]
        labels = [vehicle.vehicle_id for vehicle in event.vehicles]
        colors = [
            [0, 170, 255]
            if vehicle.vehicle_id == self._config.camera.vehicle_id
            else [255, 190, 0]
            for vehicle in event.vehicles
        ]
        fleet = f"{self._root}/fleet"
        self._recording.log(
            f"{fleet}/vehicles",
            rr.Points3D(
                positions,
                labels=labels,
                colors=colors,
                radii=[rr.Radius.ui_points(8.0)] * len(positions),
                show_labels=True,
            ),
        )
        self._recording.log(
            f"{fleet}/velocities",
            rr.Arrows3D(
                origins=positions,
                vectors=velocities,
                labels=labels,
                colors=colors,
            ),
        )
        lat_lon = [
            enu_to_geodetic(
                *vehicle.position_enu,
                self._world.georeference_origin.latitude_degrees,
                self._world.georeference_origin.longitude_degrees,
                self._world.georeference_origin.ellipsoid_height_m,
            )[:2]
            for vehicle in event.vehicles
        ]
        self._recording.log(
            f"{fleet}/geographic_positions",
            rr.GeoPoints(
                lat_lon=lat_lon,
                colors=colors,
                radii=[rr.Radius.ui_points(8.0)] * len(lat_lon),
            ),
        )
        imu_by_vehicle = {sample.vehicle_id: sample for sample in event.imu}
        self._recording.log(
            f"{fleet}/imu/linear_acceleration",
            rr.Arrows3D(
                origins=positions,
                vectors=[
                    imu_by_vehicle[vehicle.vehicle_id].linear_acceleration_mps2
                    for vehicle in event.vehicles
                ],
                colors=colors,
            ),
        )
        self._recording.log(
            f"{fleet}/imu/angular_velocity",
            rr.Arrows3D(
                origins=positions,
                vectors=[
                    imu_by_vehicle[vehicle.vehicle_id].angular_velocity_rps
                    for vehicle in event.vehicles
                ],
                colors=colors,
            ),
        )
        for vehicle in event.vehicles:
            status = (
                round(vehicle.battery_percent, 1),
                vehicle.collision_count,
                vehicle.flight_state,
                vehicle.px4_connected,
            )
            if self._last_vehicle_status.get(vehicle.vehicle_id) == status:
                continue
            self._last_vehicle_status[vehicle.vehicle_id] = status
            self._recording.log(
                f"{self._root}/vehicle/{vehicle.vehicle_id}/status",
                rr.AnyValues(
                    battery_percent=status[0],
                    collision_count=status[1],
                    flight_state=status[2],
                    px4_connected=status[3],
                ),
            )

    def _log_camera_quality(self, event: _CameraQualityEvent) -> None:
        self._set_time(event.simulation_time_s, event.physics_step)
        self._recording.log(
            f"{self._root}/vehicle/{self._config.camera.vehicle_id}/camera/down/quality",
            rr.AnyValues(
                mean_luma=event.quality.mean_luma,
                dynamic_range=event.quality.dynamic_range,
                robust_dynamic_range=event.quality.robust_dynamic_range,
                luma_standard_deviation=event.quality.luma_standard_deviation,
                non_black_fraction=event.quality.non_black_fraction,
                content=event.quality.content,
                lifecycle=event.lifecycle,
            ),
        )

    def _log_tiles(self, event: _TilesEvent) -> None:
        status = (
            event.resident_tiles,
            event.visible_tiles,
            event.loading_tiles,
            event.recovery_count,
            event.lifecycle,
        )
        if status == self._last_tiles:
            return
        self._last_tiles = status
        self._set_time(event.simulation_time_s, event.physics_step)
        self._recording.log(
            f"{self._root}/tiles",
            rr.AnyValues(
                resident=event.resident_tiles,
                visible=event.visible_tiles,
                loading=event.loading_tiles,
                recovery_count=event.recovery_count,
                lifecycle=event.lifecycle,
            ),
        )

    def _set_time(self, simulation_time_s: float, physics_step: int) -> None:
        self._recording.set_time("simulation_time", duration=simulation_time_s)
        self._recording.set_time("physics_step", sequence=physics_step)


def _recording_blueprint(config: RuntimeConfig, root: str) -> rrb.Blueprint:
    camera = f"{root}/vehicle/{config.camera.vehicle_id}/camera/down"
    fleet = f"{root}/fleet"
    map_provider = (
        rrb.MapProvider.MapboxSatellite
        if config.recording.map_provider is RecordingMapProvider.MAPBOX_SATELLITE
        else rrb.MapProvider.OpenStreetMap
    )
    return rrb.Blueprint(
        rrb.Horizontal(
            rrb.Spatial3DView(
                origin=fleet,
                contents=[f"{fleet}/vehicles", f"{fleet}/velocities"],
                name="Fleet 3D",
                line_grid=False,
            ),
            rrb.Vertical(
                rrb.Spatial2DView(
                    origin=camera,
                    contents=[camera],
                    name="Leader camera",
                ),
                rrb.MapView(
                    origin=f"{fleet}/geographic_positions",
                    contents=[f"{fleet}/geographic_positions"],
                    name="Fleet map",
                    zoom=11.0,
                    background=map_provider,
                ),
                row_shares=[0.55, 0.45],
            ),
            column_shares=[0.6, 0.4],
        ),
        auto_layout=False,
        auto_views=False,
        collapse_panels=True,
    )


def _bounded_diagnostic(error: Exception) -> str:
    message = " ".join(str(error).split())
    return f"{type(error).__name__}: {message}"[:512]
