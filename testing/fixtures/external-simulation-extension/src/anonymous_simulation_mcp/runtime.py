"""External asset publication and synthetic newest-pose production."""

from __future__ import annotations

import asyncio
import contextlib
import hashlib
import math
import time
from typing import Protocol

from veoveo_mcp.artifacts import HttpArtifactPlane
from veoveo_mcp.contract import ArtifactMetadata, PlaneCaller, PutArtifactRequest
from veoveo_mcp.simulation_pose import (
    EntityId,
    EntityPose,
    EnuPosition,
    EpochId,
    FrameRevision as PoseFrameRevision,
    LatestPosePublisher,
    PoseSnapshot,
    QuaternionXyzw as PoseQuaternionXyzw,
    SessionId,
    Sha256Digest as PoseSha256Digest,
    entity_table_digest,
)
from veoveo_mcp.simulation_view import (
    IDENTITY_TRANSFORM,
    CameraRigKind,
    GovernedArtifact,
    InterpolationPolicy,
    RendererMode,
    SceneAttribution,
    SceneDeclaration,
    SceneDeclarationBody,
    SceneEntity,
    SceneLighting,
    SceneQualityPolicy,
    VisualAssetFormat,
    VisualPrototype,
)

from .assets import ENVIRONMENT_USDA, PROTOTYPE_USDA
from .config import Config
from .contract import (
    FixtureState,
    PreparedScene,
    PrepareSceneRequest,
    ProducerLifecycle,
    ProducerState,
    StartPoseProducerRequest,
)


ENTITY_IDS = (EntityId("entity-1"), EntityId("entity-2"))
ENTITY_TABLE_REVISION = 1


class ArtifactWriter(Protocol):
    async def put(
        self,
        caller: PlaneCaller,
        request: PutArtifactRequest,
        data: bytes,
    ) -> ArtifactMetadata: ...

    async def close(self) -> None: ...


class FixtureRuntime:
    def __init__(
        self,
        config: Config,
        artifacts: ArtifactWriter | None = None,
    ) -> None:
        self._config = config
        self._artifacts = artifacts or HttpArtifactPlane(config.artifact_service_url)
        self._lock = asyncio.Lock()
        self._publisher: LatestPosePublisher | None = None
        self._publish_task: asyncio.Task[None] | None = None
        self._request: StartPoseProducerRequest | None = None
        self._diagnostic: str | None = None

    async def prepare_scene(
        self,
        caller: PlaneCaller,
        request: PrepareSceneRequest,
    ) -> PreparedScene:
        environment_metadata, prototype_metadata = await asyncio.gather(
            self._artifacts.put(
                caller,
                PutArtifactRequest(
                    mime_type="model/vnd.usd",
                    filename="anonymous-environment.usda",
                    metadata={
                        "schema": "veoveo.io/anonymous-simulation-asset/v1",
                        "role": "environment",
                    },
                ),
                ENVIRONMENT_USDA,
            ),
            self._artifacts.put(
                caller,
                PutArtifactRequest(
                    mime_type="model/vnd.usd",
                    filename="anonymous-vehicle.usda",
                    metadata={
                        "schema": "veoveo.io/anonymous-simulation-asset/v1",
                        "role": "prototype",
                    },
                ),
                PROTOTYPE_USDA,
            ),
        )
        environment = _governed(environment_metadata, ENVIRONMENT_USDA)
        prototype = _governed(prototype_metadata, PROTOTYPE_USDA)
        body = SceneDeclarationBody(
            session_id=request.session_id,
            epoch_id=request.epoch_id,
            frame_revision=request.frame_revision,
            simulation_frame=request.simulation_frame,
            environment=environment,
            prototypes=(
                VisualPrototype(
                    prototype_id="synthetic-vehicle",
                    asset=prototype,
                    local_alignment=IDENTITY_TRANSFORM,
                ),
            ),
            entities=tuple(
                SceneEntity(
                    entity_id=str(entity_id),
                    prototype_id="synthetic-vehicle",
                )
                for entity_id in ENTITY_IDS
            ),
            allowed_camera_kinds=(
                CameraRigKind.FIXED,
                CameraRigKind.LOOK_AT,
                CameraRigKind.ORBIT,
                CameraRigKind.FOLLOW_ENTITY,
                CameraRigKind.CHASE_ENTITY,
                CameraRigKind.MOUNTED_ENTITY,
                CameraRigKind.FORMATION_OVERVIEW,
            ),
            lighting=SceneLighting(
                intensity_lux=80_000.0,
                color_temperature_kelvin=6_500,
            ),
            quality=SceneQualityPolicy(
                renderer=RendererMode.RAYTRACED_LIGHTING,
                maximum_texture_dimension=4096,
                maximum_asset_bytes=(
                    environment.byte_length + prototype.byte_length
                ),
                interpolation=InterpolationPolicy.LINEAR,
                maximum_pose_age_ms=500,
            ),
            attribution=(
                SceneAttribution(
                    source="Anonymous external Simulation View acceptance fixture",
                    license="CC0-1.0",
                    attribution_url=(
                        "https://creativecommons.org/publicdomain/zero/1.0/"
                    ),
                ),
            ),
        )
        return PreparedScene(
            scene=SceneDeclaration.from_body(body),
            producer_id=self._config.producer_id,
            producer_spiffe_id=self._config.producer_spiffe_id,
            entity_table_revision=ENTITY_TABLE_REVISION,
            entity_table_digest=str(
                entity_table_digest(ENTITY_TABLE_REVISION, ENTITY_IDS)
            ),
        )

    async def start(self, request: StartPoseProducerRequest) -> ProducerState:
        async with self._lock:
            if self._publish_task is not None and not self._publish_task.done():
                if request != self._request:
                    raise ValueError(
                        "the fixture already publishes another session or epoch"
                    )
                return self._state_unlocked()
            publisher = LatestPosePublisher(self._config.pose_tls)
            self._publisher = publisher
            self._request = request
            self._diagnostic = None
            self._publish_task = asyncio.create_task(
                self._publish(request, publisher),
                name="anonymous-simulation-pose",
            )
            return self._state_unlocked()

    async def stop(self, session_id: str | None = None) -> ProducerState:
        async with self._lock:
            if (
                session_id is not None
                and self._request is not None
                and session_id != self._request.session_id
            ):
                raise ValueError("pose producer session identity does not match")
            task = self._publish_task
            publisher = self._publisher
            self._publish_task = None
            self._publisher = None
            self._request = None
        if task is not None:
            task.cancel()
            with contextlib.suppress(asyncio.CancelledError):
                await task
        if publisher is not None:
            await asyncio.to_thread(publisher.close)
        return await self.state()

    async def state(self) -> ProducerState:
        async with self._lock:
            return self._state_unlocked()

    async def fixture_state(self) -> FixtureState:
        return FixtureState(producer=await self.state())

    async def close(self) -> None:
        await self.stop()
        await self._artifacts.close()

    def _state_unlocked(self) -> ProducerState:
        request = self._request
        publisher = self._publisher
        if publisher is None:
            return ProducerState(
                lifecycle=ProducerLifecycle.STOPPED,
                producer_id=self._config.producer_id,
                producer_spiffe_id=self._config.producer_spiffe_id,
                diagnostic=self._diagnostic,
            )
        status = publisher.status()
        lifecycle = (
            ProducerLifecycle.RUNNING
            if status.connected and status.sent_snapshots > 0
            else ProducerLifecycle.DEGRADED
            if status.last_error is not None
            else ProducerLifecycle.STARTING
        )
        return ProducerState(
            lifecycle=lifecycle,
            producer_id=self._config.producer_id,
            producer_spiffe_id=self._config.producer_spiffe_id,
            session_id=request.session_id if request is not None else None,
            epoch_id=request.epoch_id if request is not None else None,
            cadence_hz=request.cadence_hz if request is not None else None,
            offered_snapshots=status.offered_snapshots,
            sent_snapshots=status.sent_snapshots,
            replaced_snapshots=status.replaced_snapshots,
            last_sent_sequence=status.last_sent_sequence,
            diagnostic=status.last_error or self._diagnostic,
        )

    async def _publish(
        self,
        request: StartPoseProducerRequest,
        publisher: LatestPosePublisher,
    ) -> None:
        sequence = 0
        started_ns = time.monotonic_ns()
        interval = 1.0 / request.cadence_hz
        try:
            while True:
                sequence += 1
                elapsed_ns = time.monotonic_ns() - started_ns
                publisher.offer(_snapshot(request, sequence, elapsed_ns))
                await asyncio.sleep(interval)
        except asyncio.CancelledError:
            raise
        except Exception as error:  # noqa: BLE001 - state must expose producer failure
            async with self._lock:
                self._diagnostic = f"{type(error).__name__}: {error}"


def _governed(metadata: ArtifactMetadata, payload: bytes) -> GovernedArtifact:
    if metadata.byte_len != len(payload):
        raise ValueError("artifact plane returned a mismatched byte length")
    return GovernedArtifact(
        artifact_uri=metadata.artifact_uri,
        digest=f"sha256:{hashlib.sha256(payload).hexdigest()}",
        format=VisualAssetFormat.USD,
        byte_length=metadata.byte_len,
    )


def _snapshot(
    request: StartPoseProducerRequest,
    sequence: int,
    elapsed_ns: int,
) -> PoseSnapshot:
    elapsed_seconds = elapsed_ns / 1_000_000_000
    poses: list[EntityPose] = []
    for index, entity_id in enumerate(ENTITY_IDS):
        phase = elapsed_seconds * 0.45 + index * math.pi
        yaw = phase + math.pi / 2.0
        poses.append(
            EntityPose(
                entity_id=entity_id,
                position=EnuPosition(
                    east_m=8.0 * math.cos(phase),
                    north_m=8.0 * math.sin(phase),
                    up_m=2.0 + index * 1.5,
                ),
                orientation=PoseQuaternionXyzw(
                    x=0.0,
                    y=0.0,
                    z=math.sin(yaw / 2.0),
                    w=math.cos(yaw / 2.0),
                ),
            )
        )
    return PoseSnapshot.build(
        session_id=SessionId(request.session_id),
        epoch_id=EpochId(request.epoch_id),
        sequence=sequence,
        simulation_timestamp_ns=elapsed_ns,
        frame_revision=PoseFrameRevision(
            uri=request.frame_revision.uri,
            digest=PoseSha256Digest(request.frame_revision.digest),
        ),
        entity_table_revision=ENTITY_TABLE_REVISION,
        entities=poses,
    )
