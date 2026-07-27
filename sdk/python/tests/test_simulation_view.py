import json
import uuid
from pathlib import Path

import pytest
from pydantic import ValidationError

from veoveo_mcp.simulation_view import (
    IDENTITY_TRANSFORM,
    CameraRigKind,
    FrameRevision,
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


def _artifact(index: int, byte_length: int) -> GovernedArtifact:
    artifact_id = uuid.UUID(f"01900000-0000-7000-8000-{index:012x}")
    return GovernedArtifact(
        artifact_uri=f"artifact://{artifact_id}",
        digest=f"sha256:{index:064x}",
        format=VisualAssetFormat.USD,
        byte_length=byte_length,
    )


def _body() -> SceneDeclarationBody:
    revision = "frames://world/synthetic/revision/revision-1"
    return SceneDeclarationBody(
        session_id="anonymous-session",
        epoch_id="epoch-1",
        frame_revision=FrameRevision(
            uri=revision,
            digest="sha256:" + "11" * 32,
        ),
        simulation_frame=f"{revision}/frame/simulation",
        environment=_artifact(1, 1024),
        prototypes=(
            VisualPrototype(
                prototype_id="vehicle",
                asset=_artifact(2, 2048),
                local_alignment=IDENTITY_TRANSFORM,
            ),
        ),
        entities=(
            SceneEntity(entity_id="entity-1", prototype_id="vehicle"),
            SceneEntity(entity_id="entity-2", prototype_id="vehicle"),
        ),
        allowed_camera_kinds=(
            CameraRigKind.FOLLOW_ENTITY,
            CameraRigKind.ORBIT,
            CameraRigKind.FORMATION_OVERVIEW,
        ),
        lighting=SceneLighting(
            intensity_lux=80_000.0,
            color_temperature_kelvin=6_500,
        ),
        quality=SceneQualityPolicy(
            renderer=RendererMode.RAYTRACED_LIGHTING,
            maximum_texture_dimension=4096,
            maximum_asset_bytes=4096,
            interpolation=InterpolationPolicy.LINEAR,
            maximum_pose_age_ms=500,
        ),
        attribution=(
            SceneAttribution(
                source="Anonymous Simulation View fixture",
                license="CC0-1.0",
                attribution_url="https://creativecommons.org/publicdomain/zero/1.0/",
            ),
        ),
    )


def test_scene_digest_matches_rust_field_order_fixture() -> None:
    fixture = (
        Path(__file__).resolve().parents[3]
        / "platform"
        / "simulation"
        / "fixtures"
        / "anonymous-scene-body.json"
    )
    body = SceneDeclarationBody.model_validate(json.loads(fixture.read_text()))
    assert body == _body()
    declaration = SceneDeclaration.from_body(body)
    assert declaration.digest == (
        "sha256:67291c10c39898b2ea11ac9bbe12643148b0112bd868ed44579aaa818fea48e4"
    )
    assert list(declaration.wire()["body"]) == [
        "schemaVersion",
        "sessionId",
        "epochId",
        "frameRevision",
        "simulationFrame",
        "environment",
        "prototypes",
        "entities",
        "allowedCameraKinds",
        "lighting",
        "quality",
        "attribution",
    ]


def test_scene_declaration_rejects_cross_revision_or_unknown_prototypes() -> None:
    body = _body()
    value = body.model_dump()
    value["simulation_frame"] = (
        "frames://world/other/revision/revision-2/frame/simulation"
    )
    with pytest.raises(ValidationError, match="must belong"):
        SceneDeclarationBody.model_validate(value)

    value = body.model_dump()
    value["entities"][0]["prototype_id"] = "missing"
    with pytest.raises(ValidationError, match="declared prototype"):
        SceneDeclarationBody.model_validate(value)


def test_scene_digest_rejects_mutation() -> None:
    declaration = SceneDeclaration.from_body(_body())
    with pytest.raises(ValidationError, match="digest does not match"):
        SceneDeclaration(body=declaration.body, digest="sha256:" + "ff" * 32)
