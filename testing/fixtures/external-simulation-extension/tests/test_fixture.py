from __future__ import annotations

from datetime import datetime, timezone
from pathlib import Path
from typing import cast

import pytest

from veoveo_mcp.contract import ArtifactMetadata, PlaneCaller
from veoveo_mcp.simulation_pose import PoseTlsConfig
from veoveo_mcp.simulation_view import FrameRevision

from anonymous_simulation_mcp.assets import ENVIRONMENT_USDA, PROTOTYPE_USDA
from anonymous_simulation_mcp.config import Config
from anonymous_simulation_mcp.contract import (
    PrepareSceneRequest,
    StartPoseProducerRequest,
)
from anonymous_simulation_mcp.runtime import (
    ENTITY_IDS,
    ENTITY_TABLE_REVISION,
    FixtureRuntime,
    _snapshot,
)


class FakeArtifactPlane:
    def __init__(self) -> None:
        self.uploads: list[tuple[object, bytes]] = []
        self.closed = False

    async def put(
        self,
        _caller: PlaneCaller,
        request: object,
        data: bytes,
    ) -> ArtifactMetadata:
        self.uploads.append((request, data))
        index = len(self.uploads)
        artifact_id = f"01900000-0000-7000-8000-{index:012x}"
        return ArtifactMetadata(
            artifact_id=artifact_id,
            artifact_uri=f"artifact://{artifact_id}",
            byte_len=len(data),
            mime_type="model/vnd.usd",
            filename=f"fixture-{index}.usda",
            created_at=datetime.now(timezone.utc),
        )

    async def close(self) -> None:
        self.closed = True


def _config() -> Config:
    return Config(
        port=8812,
        allowed_hosts=("anonymous-simulation-mcp:8812",),
        internal_trust_jwks='{"keys":[]}',
        artifact_service_url="http://artifact-service:8790",
        producer_id="anonymous-synthetic",
        producer_spiffe_id=(
            "spiffe://veoveo.local/simulation/anonymous-synthetic"
        ),
        pose_tls=PoseTlsConfig(
            host="simulation-view-pose",
            port=7443,
            server_hostname="simulation-view-pose.veoveo.svc",
            ca_certificate=Path("/run/secrets/simulation-view-pose/ca.crt"),
            client_certificate=Path(
                "/run/secrets/simulation-view-pose/tls.crt"
            ),
            client_private_key=Path(
                "/run/secrets/simulation-view-pose/tls.key"
            ),
        ),
    )


def _identity_request() -> PrepareSceneRequest:
    revision = "frames://world/synthetic/revision/revision-1"
    return PrepareSceneRequest(
        session_id="anonymous-session",
        epoch_id="epoch-1",
        frame_revision=FrameRevision(
            uri=revision,
            digest="sha256:" + "11" * 32,
        ),
        simulation_frame=f"{revision}/frame/simulation",
    )


@pytest.mark.asyncio
async def test_fixture_publishes_owned_assets_and_returns_a_core_scene() -> None:
    plane = FakeArtifactPlane()
    runtime = FixtureRuntime(_config(), plane)
    prepared = await runtime.prepare_scene(
        cast(PlaneCaller, object()),
        _identity_request(),
    )

    assert [payload for _, payload in plane.uploads] == [
        ENVIRONMENT_USDA,
        PROTOTYPE_USDA,
    ]
    assert prepared.scene.body.session_id == "anonymous-session"
    assert prepared.scene.body.entities[0].entity_id == "entity-1"
    assert prepared.scene.body.allowed_camera_kinds
    assert prepared.scene.digest.startswith("sha256:")
    assert prepared.entity_table_revision == ENTITY_TABLE_REVISION
    assert prepared.entity_table_digest.startswith("sha256:")
    await runtime.close()
    assert plane.closed


def test_complete_snapshots_move_every_declared_entity() -> None:
    identity = _identity_request()
    request = StartPoseProducerRequest(
        **identity.model_dump(),
        cadence_hz=30,
    )
    first = _snapshot(request, 1, 0)
    second = _snapshot(request, 2, 1_000_000_000)

    assert first.sequence == 1
    assert second.sequence == 2
    assert tuple(entity.entity_id for entity in first.entities) == ENTITY_IDS
    assert first.entity_table_digest == second.entity_table_digest
    assert first.entities[0].position != second.entities[0].position


def test_assets_are_declarative_self_contained_usda() -> None:
    for payload in (ENVIRONMENT_USDA, PROTOTYPE_USDA):
        lower = payload.lower()
        assert payload.startswith(b"#usda 1.0")
        assert b"@" not in payload
        for forbidden in (
            b"http:",
            b"https:",
            b"omniverse:",
            b"file:",
            b"python",
            b"script",
            b"physics",
            b"physx",
            b".so",
            b".dll",
        ):
            assert forbidden not in lower
