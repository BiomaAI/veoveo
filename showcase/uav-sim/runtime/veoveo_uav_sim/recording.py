from __future__ import annotations

import json
import logging
import queue
import threading
import time
import uuid
from dataclasses import dataclass

import rerun as rr
import rerun.blueprint as rrb

from .config import RecordingMapProvider, RuntimeConfig
from .event_queue import NonBlockingEventQueue
from .geo import enu_to_geodetic
from .h264 import NativeH264AccessUnit
from .recording_segments import RecordingSegmentBudget, new_recording_key
from .state import VehicleTelemetry
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
    recording_key: str
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
class _TilesEvent:
    resident_tiles: int
    visible_tiles: int
    loading_tiles: int
    refresh_count: int
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
    | _TilesEvent
    | _MissionEvent
    | _StopEvent
)


class RecordedH264CameraStream:
    """Publish one native Isaac NVENC access unit to governed Recording."""

    def __init__(
        self,
        recording: rr.RecordingStream,
        entity_path: str,
        width: int,
        height: int,
    ) -> None:
        self._recording = recording
        self._entity_path = entity_path
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
        self._set_time(simulation_time_s, physics_step)
        self._recording.log(
            self._entity_path,
            _video_packet(access_unit.sample),
        )

    def _set_time(self, simulation_time_s: float, physics_step: int) -> None:
        self._recording.set_time("simulation_time", duration=simulation_time_s)
        self._recording.set_time("physics_step", sequence=physics_step)


def _video_packet(sample: bytes) -> rr.VideoStream:
    return rr.VideoStream.from_fields(sample=sample)


class RecordingPublisher:
    """Nonblocking simulation-side facade over one retrying recording worker."""

    def __init__(
        self,
        config: RuntimeConfig,
        world: WorldConfiguration,
        recording_key: uuid.UUID,
    ) -> None:
        self._config = config
        self._world = world
        self._initial_recording_key = recording_key
        self._events = NonBlockingEventQueue[_RecordingEvent](
            config.recording.queue_capacity
        )
        self._status_lock = threading.Lock()
        self._lifecycle = "connecting"
        self._recording_key = str(recording_key)
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
        with self._status_lock:
            return self._recording_key

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

    def log_tiles(
        self,
        resident_tiles: int,
        visible_tiles: int,
        loading_tiles: int,
        refresh_count: int,
        lifecycle: str,
        simulation_time_s: float,
        physics_step: int,
    ) -> None:
        self._events.offer(
            _TilesEvent(
                resident_tiles=resident_tiles,
                visible_tiles=visible_tiles,
                loading_tiles=loading_tiles,
                refresh_count=refresh_count,
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
                recording_key=self._recording_key,
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
        recording_key = self._initial_recording_key
        while not self._closed.is_set():
            sink: _RecordingSink | None = None
            try:
                sink = _RecordingSink(self._config, self._world, recording_key)
                self._set_status("ready", None, recording_key)
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
                    if sink.should_rotate_before(event):
                        previous_key = recording_key
                        sink.close()
                        recording_key = new_recording_key()
                        sink = _RecordingSink(
                            self._config, self._world, recording_key
                        )
                        self._set_status("ready", None, recording_key)
                        LOGGER.info(
                            "rotated governed recording %s to %s at its bounded segment policy",
                            previous_key,
                            recording_key,
                        )
                    sink.handle(event)
            except Exception as error:
                message = _bounded_diagnostic(error)
                LOGGER.exception(
                    "governed recording worker failed; simulation continues"
                )
                self._set_status("degraded", message)
                if sink is not None:
                    sink.abort()
                recording_key = new_recording_key()
                if self._closed.wait(2.0):
                    return

    def _set_status(
        self,
        lifecycle: str,
        error: str | None,
        recording_key: uuid.UUID | None = None,
    ) -> None:
        with self._status_lock:
            self._lifecycle = lifecycle
            self._last_error = error
            if recording_key is not None:
                self._recording_key = str(recording_key)


class _RecordingSink:
    def __init__(
        self,
        config: RuntimeConfig,
        world: WorldConfiguration,
        recording_key: uuid.UUID,
    ) -> None:
        self._config = config
        self._world = world
        self._root = f"/world/uav-sim/{config.session_id}"
        self._recording = rr.RecordingStream(
            "veoveo-uav-sim",
            recording_id=recording_key,
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
        self._camera = RecordedH264CameraStream(
            self._recording,
            camera_entity,
            config.camera.width,
            config.camera.height,
        )
        self._budget = RecordingSegmentBudget(
            maximum_bytes=config.recording.maximum_segment_bytes,
            maximum_seconds=config.recording.maximum_segment_seconds,
            opened_monotonic_s=time.monotonic(),
        )

    def should_rotate_before(
        self,
        event: _FrameEvent | _CameraEvent | _TilesEvent | _MissionEvent,
    ) -> bool:
        return self._budget.should_rotate_before(
            _recording_event_budget_bytes(event), time.monotonic()
        )

    def handle(
        self,
        event: _FrameEvent
        | _CameraEvent
        | _TilesEvent
        | _MissionEvent,
    ) -> None:
        payload_bytes = _recording_event_budget_bytes(event)
        if isinstance(event, _FrameEvent):
            self._log_frame(event)
        elif isinstance(event, _CameraEvent):
            self._camera.publish(
                event.access_unit, event.simulation_time_s, event.physics_step
            )
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
        self._budget.account(payload_bytes)

    def close(self) -> None:
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

    def _log_tiles(self, event: _TilesEvent) -> None:
        status = (
            event.resident_tiles,
            event.visible_tiles,
            event.loading_tiles,
            event.refresh_count,
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
                refresh_count=event.refresh_count,
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


def _recording_event_budget_bytes(
    event: _FrameEvent | _CameraEvent | _TilesEvent | _MissionEvent,
) -> int:
    envelope_bytes = 4 * 1024
    if isinstance(event, _CameraEvent):
        return envelope_bytes + len(event.access_unit.sample)
    if isinstance(event, _FrameEvent):
        return envelope_bytes + len(event.vehicles) * 1024 + len(event.imu) * 512
    if isinstance(event, _MissionEvent):
        return (
            envelope_bytes
            + len(event.mission_id.encode())
            + len(event.lifecycle.encode())
            + len(event.detail_json.encode())
        )
    return envelope_bytes


def _bounded_diagnostic(error: Exception) -> str:
    message = " ".join(str(error).split())
    return f"{type(error).__name__}: {message}"[:512]
