"""Typed MCP inputs and outputs for the synthetic external producer."""

from __future__ import annotations

from enum import Enum

from pydantic import BaseModel, ConfigDict, Field
from pydantic.alias_generators import to_camel

from veoveo_mcp.simulation_view import (
    FrameRevision,
    Identifier,
    SceneDeclaration,
    Sha256Digest,
    WorldFrameUri,
)


class WireModel(BaseModel):
    model_config = ConfigDict(
        alias_generator=to_camel,
        populate_by_name=True,
        extra="forbid",
    )


class SceneIdentity(WireModel):
    session_id: Identifier
    epoch_id: Identifier
    frame_revision: FrameRevision
    simulation_frame: WorldFrameUri


class PrepareSceneRequest(SceneIdentity):
    pass


class StartPoseProducerRequest(SceneIdentity):
    cadence_hz: int = Field(default=30, ge=1, le=120)


class StopPoseProducerRequest(WireModel):
    session_id: Identifier


class GetFixtureStateRequest(WireModel):
    pass


class PreparedScene(WireModel):
    scene: SceneDeclaration
    producer_id: Identifier
    producer_spiffe_id: str
    entity_table_revision: int
    entity_table_digest: Sha256Digest


class ProducerLifecycle(str, Enum):
    STOPPED = "stopped"
    STARTING = "starting"
    RUNNING = "running"
    DEGRADED = "degraded"


class ProducerState(WireModel):
    lifecycle: ProducerLifecycle
    producer_id: Identifier
    producer_spiffe_id: str
    session_id: Identifier | None = None
    epoch_id: Identifier | None = None
    cadence_hz: int | None = None
    offered_snapshots: int = 0
    sent_snapshots: int = 0
    replaced_snapshots: int = 0
    last_sent_sequence: int | None = None
    diagnostic: str | None = None


class FixtureState(WireModel):
    schema_version: str = "veoveo.io/anonymous-simulation-fixture/v1"
    producer: ProducerState
