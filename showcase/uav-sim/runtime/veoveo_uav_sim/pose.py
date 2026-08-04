from __future__ import annotations

import math
import threading
import time
from collections import deque
from collections.abc import Callable, Sequence
from typing import TYPE_CHECKING, Any

from veoveo_mcp.simulation_pose import (
    POSE_PROTOCOL_SCHEMA,
    EntityId,
    EntityPose,
    EnuPosition,
    EpochId,
    FrameRevision,
    LatestPosePublisher,
    PoseSnapshot,
    PoseTlsConfig,
    QuaternionXyzw,
    SessionId,
    Sha256Digest,
    entity_table_digest,
)

from .config import PosePublisherConfig
from .world_config import WorldConfiguration

if TYPE_CHECKING:
    from .state import VehicleTelemetry


PoseStateCallback = Callable[[dict[str, Any]], None]


class PhysicsCadenceGate:
    """Select an exact rational cadence from monotonically advancing physics steps."""

    def __init__(self, physics_hz: int, output_hz: int) -> None:
        if physics_hz < 1 or output_hz < 1 or output_hz > physics_hz:
            raise ValueError("physics/output cadence is invalid")
        self._physics_hz = physics_hz
        self._output_hz = output_hz
        self._last_step = 0

    def due(self, physics_step: int) -> bool:
        if physics_step <= self._last_step:
            raise RuntimeError("physics cadence steps must increase monotonically")
        previous_bucket = ((physics_step - 1) * self._output_hz) // self._physics_hz
        current_bucket = (physics_step * self._output_hz) // self._physics_hz
        self._last_step = physics_step
        return current_bucket > previous_bucket

    def reset(self) -> None:
        self._last_step = 0


def entity_ids(vehicle_count: int) -> tuple[EntityId, ...]:
    return tuple(EntityId(f"uav-{index + 1}") for index in range(vehicle_count))


def initial_pose_publication(
    config: PosePublisherConfig,
    vehicle_count: int,
    cadence_hz: int,
) -> dict[str, Any]:
    identities = entity_ids(vehicle_count)
    return {
        "protocol_schema": POSE_PROTOCOL_SCHEMA,
        "producer_id": config.producer_id,
        "producer_spiffe_id": config.producer_spiffe_id,
        "epoch_id": config.epoch_id,
        "entity_table_revision": config.entity_table_revision,
        "entity_table_digest": str(
            entity_table_digest(config.entity_table_revision, identities)
        ),
        "cadence_hz": cadence_hz,
        "lifecycle": "starting",
        "offered_snapshots": 0,
        "sent_snapshots": 0,
        "replaced_snapshots": 0,
    }


class PoseProducer:
    def __init__(
        self,
        *,
        config: PosePublisherConfig,
        session_id: str,
        world: WorldConfiguration,
        vehicle_count: int,
        cadence_hz: int,
        buffer_duration_ms: int,
        update_state: PoseStateCallback,
    ) -> None:
        self._config = config
        self._session_id = SessionId(session_id)
        self._epoch_id = EpochId(config.epoch_id)
        self._frame_revision = FrameRevision(
            world.revision_uri,
            Sha256Digest(f"sha256:{world.spec_sha256}"),
        )
        self._entity_ids = entity_ids(vehicle_count)
        self._entity_table_digest = entity_table_digest(
            config.entity_table_revision, self._entity_ids
        )
        self._cadence_hz = cadence_hz
        self._timestamp_increment_ns = round(1_000_000_000 / cadence_hz)
        self._sequence = 0
        self._simulation_timestamp_ns = 0
        self._update_state = update_state
        self._last_publication: dict[str, Any] | None = None
        self._buffer_snapshots = max(
            2, math.ceil(cadence_hz * buffer_duration_ms / 1_000)
        )
        self._maximum_queued_snapshots = self._buffer_snapshots * 2
        self._condition = threading.Condition()
        self._queue: deque[PoseSnapshot] = deque()
        self._closing = False
        self._emission_started = False
        self._publisher = LatestPosePublisher(
            PoseTlsConfig(
                host=config.ingress_host,
                port=config.ingress_port,
                server_hostname=config.server_hostname,
                ca_certificate=config.ca_certificate,
                client_certificate=config.client_certificate,
                client_private_key=config.client_private_key,
            ),
            thread_name=f"uav-pose-{config.producer_id}",
        )
        self._emitter = threading.Thread(
            target=self._emit,
            name=f"uav-pose-cadence-{config.producer_id}",
            daemon=True,
        )
        self._emitter.start()
        self.poll()

    def offer(self, telemetry: Sequence[VehicleTelemetry]) -> None:
        by_id = {vehicle.vehicle_id: vehicle for vehicle in telemetry}
        if set(by_id) != {identity.value for identity in self._entity_ids}:
            raise RuntimeError(
                "pose publication requires one complete snapshot for every UAV entity"
            )
        self._sequence += 1
        self._simulation_timestamp_ns += self._timestamp_increment_ns
        entities = tuple(
            self._entity_pose(identity, by_id[identity.value])
            for identity in self._entity_ids
        )
        snapshot = PoseSnapshot(
            session_id=self._session_id,
            epoch_id=self._epoch_id,
            sequence=self._sequence,
            simulation_timestamp_ns=self._simulation_timestamp_ns,
            frame_revision=self._frame_revision,
            entity_table_revision=self._config.entity_table_revision,
            entity_table_digest=self._entity_table_digest,
            entities=entities,
        )
        with self._condition:
            self._condition.wait_for(
                lambda: self._closing
                or len(self._queue) < self._maximum_queued_snapshots
            )
            if self._closing:
                raise RuntimeError("pose producer closed while accepting a snapshot")
            self._queue.append(snapshot)
            self._condition.notify_all()

    def poll(self) -> None:
        status = self._publisher.status()
        if not status.running:
            lifecycle = "failed"
        elif status.connected and status.sent_snapshots > 0:
            lifecycle = "ready"
        elif status.sent_snapshots > 0:
            lifecycle = "degraded"
        else:
            lifecycle = "connecting"
        publication = initial_pose_publication(
            self._config, len(self._entity_ids), self._cadence_hz
        )
        publication.update(
            lifecycle=lifecycle,
            offered_snapshots=status.offered_snapshots,
            sent_snapshots=status.sent_snapshots,
            replaced_snapshots=status.replaced_snapshots,
        )
        with self._condition:
            publication["queued_snapshots"] = len(self._queue)
            publication["buffer_target_snapshots"] = self._buffer_snapshots
        if status.last_sent_sequence is not None:
            publication["last_sent_sequence"] = status.last_sent_sequence
        if status.last_error is not None:
            publication["diagnostic"] = status.last_error
        self._publish_state(publication)

    def close(self) -> None:
        with self._condition:
            self._closing = True
            self._condition.notify_all()
        self._emitter.join(timeout=5.0)
        if self._emitter.is_alive():
            raise RuntimeError("pose cadence emitter did not stop")
        self._publisher.close()
        publication = initial_pose_publication(
            self._config, len(self._entity_ids), self._cadence_hz
        )
        status = self._publisher.status()
        publication.update(
            lifecycle="stopped",
            offered_snapshots=status.offered_snapshots,
            sent_snapshots=status.sent_snapshots,
            replaced_snapshots=status.replaced_snapshots,
        )
        if status.last_sent_sequence is not None:
            publication["last_sent_sequence"] = status.last_sent_sequence
        self._publish_state(publication)

    def _emit(self) -> None:
        period = 1.0 / self._cadence_hz
        deadline = time.monotonic()
        while True:
            with self._condition:
                self._condition.wait_for(
                    lambda: self._closing
                    or (
                        len(self._queue)
                        >= (self._buffer_snapshots if not self._emission_started else 1)
                    )
                )
                if self._closing:
                    return
                if not self._emission_started:
                    self._emission_started = True
                    deadline = time.monotonic()
                snapshot = self._queue.popleft()
                self._condition.notify_all()

            remaining = deadline - time.monotonic()
            if remaining > 0:
                with self._condition:
                    self._condition.wait_for(lambda: self._closing, timeout=remaining)
                    if self._closing:
                        return
            elif remaining < -period:
                deadline = time.monotonic()
            if self._closing:
                return
            self._publisher.offer(snapshot)
            self.poll()
            deadline += period

    def _publish_state(self, publication: dict[str, Any]) -> None:
        if publication != self._last_publication:
            self._update_state(publication)
            self._last_publication = publication

    @staticmethod
    def _entity_pose(
        entity_id: EntityId, telemetry: VehicleTelemetry
    ) -> EntityPose:
        east, north, up = telemetry.position_enu
        x, y, z, w = telemetry.attitude_xyzw
        return EntityPose(
            entity_id=entity_id,
            position=EnuPosition(east_m=east, north_m=north, up_m=up),
            orientation=QuaternionXyzw(x=x, y=y, z=z, w=w),
            active=True,
            visible=True,
        )
