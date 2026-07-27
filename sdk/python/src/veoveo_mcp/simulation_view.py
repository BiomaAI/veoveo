"""Typed scene declarations for the provider-neutral Simulation View service.

External simulation extensions own their scene declarations and governed
visual assets. This module gives Python producers the exact
``veoveo.io/simulation-view-scene/v1`` shape and the same deterministic
SHA-256 body digest as the Rust control plane.
"""

from __future__ import annotations

import hashlib
import json
import math
import uuid
from enum import Enum
from typing import Annotated, Self

from pydantic import (
    AfterValidator,
    BaseModel,
    ConfigDict,
    Field,
    PositiveFloat,
    PositiveInt,
    model_validator,
)
from pydantic.alias_generators import to_camel


SCENE_SCHEMA = "veoveo.io/simulation-view-scene/v1"
_IDENTITY_CHARACTERS = frozenset(
    "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_."
)


def _identifier(value: str) -> str:
    if (
        not isinstance(value, str)
        or not 1 <= len(value) <= 128
        or any(character not in _IDENTITY_CHARACTERS for character in value)
    ):
        raise ValueError(
            "identifier must contain 1-128 ASCII letters, digits, dashes, "
            "underscores, or periods"
        )
    return value


def _sha256_digest(value: str) -> str:
    if (
        not isinstance(value, str)
        or len(value) != 71
        or not value.startswith("sha256:")
        or any(character not in "0123456789abcdef" for character in value[7:])
    ):
        raise ValueError(
            "SHA-256 digest must be lowercase sha256:<64 hexadecimal characters>"
        )
    return value


def _artifact_uri(value: str) -> str:
    prefix = "artifact://"
    if not isinstance(value, str) or not value.startswith(prefix):
        raise ValueError("artifact URI must use artifact://{uuidv7}")
    try:
        artifact_id = uuid.UUID(value.removeprefix(prefix))
    except ValueError as error:
        raise ValueError("artifact URI must use artifact://{uuidv7}") from error
    if artifact_id.version != 7 or str(artifact_id) != value.removeprefix(prefix):
        raise ValueError("artifact URI must use canonical lowercase UUIDv7")
    return value


def _frame_revision_uri(value: str) -> str:
    if (
        not isinstance(value, str)
        or not value.startswith("frames://world/")
        or "/revision/" not in value
        or len(value.encode()) > 512
    ):
        raise ValueError("frame revision must be a bounded frames://world/ URI")
    return value


def _world_frame_uri(value: str) -> str:
    if (
        not isinstance(value, str)
        or not value.startswith("frames://world/")
        or "/revision/" not in value
        or "/frame/" not in value
        or len(value.encode()) > 768
    ):
        raise ValueError("simulation frame must be a revision-scoped Frames URI")
    return value


Identifier = Annotated[str, AfterValidator(_identifier)]
Sha256Digest = Annotated[str, AfterValidator(_sha256_digest)]
ArtifactUri = Annotated[str, AfterValidator(_artifact_uri)]
FrameRevisionUri = Annotated[str, AfterValidator(_frame_revision_uri)]
WorldFrameUri = Annotated[str, AfterValidator(_world_frame_uri)]


class ContractModel(BaseModel):
    model_config = ConfigDict(
        alias_generator=to_camel,
        populate_by_name=True,
        extra="forbid",
        frozen=True,
    )


class VisualAssetFormat(str, Enum):
    USD = "usd"
    USDZ = "usdz"
    GLB = "glb"
    GLTF = "gltf"


class CameraRigKind(str, Enum):
    FIXED = "fixed"
    LOOK_AT = "look_at"
    ORBIT = "orbit"
    FOLLOW_ENTITY = "follow_entity"
    CHASE_ENTITY = "chase_entity"
    MOUNTED_ENTITY = "mounted_entity"
    FORMATION_OVERVIEW = "formation_overview"


class RendererMode(str, Enum):
    RAYTRACED_LIGHTING = "raytraced_lighting"


class InterpolationPolicy(str, Enum):
    HOLD_LATEST = "hold_latest"
    LINEAR = "linear"


class Vector3(ContractModel):
    x: float
    y: float
    z: float

    @model_validator(mode="after")
    def finite(self) -> Self:
        if not all(math.isfinite(value) for value in (self.x, self.y, self.z)):
            raise ValueError("vector components must be finite")
        return self


class QuaternionXyzw(ContractModel):
    x: float
    y: float
    z: float
    w: float

    @model_validator(mode="after")
    def normalized(self) -> Self:
        components = (self.x, self.y, self.z, self.w)
        if not all(math.isfinite(value) for value in components):
            raise ValueError("quaternion components must be finite")
        if abs(sum(value * value for value in components) - 1.0) > 1.0e-3:
            raise ValueError("quaternion must be normalized")
        return self


class LocalTransform(ContractModel):
    translation_m: Vector3
    orientation_xyzw: QuaternionXyzw
    scale: Vector3

    @model_validator(mode="after")
    def positive_scale(self) -> Self:
        if min(self.scale.x, self.scale.y, self.scale.z) <= 0.0:
            raise ValueError("local transform scale must be positive")
        return self


class FrameRevision(ContractModel):
    uri: FrameRevisionUri
    digest: Sha256Digest


class GovernedArtifact(ContractModel):
    artifact_uri: ArtifactUri
    digest: Sha256Digest
    format: VisualAssetFormat
    byte_length: PositiveInt


class VisualPrototype(ContractModel):
    prototype_id: Identifier
    asset: GovernedArtifact
    local_alignment: LocalTransform


class SceneEntity(ContractModel):
    entity_id: Identifier
    prototype_id: Identifier
    static_transform: LocalTransform | None = None


class SceneAttribution(ContractModel):
    source: str = Field(min_length=1, max_length=1_024)
    license: str = Field(min_length=1, max_length=1_024)
    attribution_url: str | None = Field(default=None, max_length=2_048)

    @model_validator(mode="after")
    def valid_attribution(self) -> Self:
        if self.source.strip() != self.source or self.license.strip() != self.license:
            raise ValueError("scene attribution fields must be trimmed")
        if self.attribution_url is not None and not self.attribution_url.startswith(
            "https://"
        ):
            raise ValueError("scene attribution URL must use HTTPS")
        return self


class SceneLighting(ContractModel):
    intensity_lux: PositiveFloat
    color_temperature_kelvin: int = Field(ge=1_000, le=20_000)


class SceneQualityPolicy(ContractModel):
    renderer: RendererMode
    maximum_texture_dimension: PositiveInt
    maximum_asset_bytes: PositiveInt
    interpolation: InterpolationPolicy
    maximum_pose_age_ms: PositiveInt


class SceneDeclarationBody(ContractModel):
    schema_version: str = Field(default=SCENE_SCHEMA)
    session_id: Identifier
    epoch_id: Identifier
    frame_revision: FrameRevision
    simulation_frame: WorldFrameUri
    environment: GovernedArtifact
    prototypes: tuple[VisualPrototype, ...] = Field(min_length=1)
    entities: tuple[SceneEntity, ...] = Field(min_length=1)
    allowed_camera_kinds: tuple[CameraRigKind, ...] = Field(min_length=1)
    lighting: SceneLighting
    quality: SceneQualityPolicy
    attribution: tuple[SceneAttribution, ...] = Field(min_length=1)

    @model_validator(mode="after")
    def valid_scene(self) -> Self:
        if self.schema_version != SCENE_SCHEMA:
            raise ValueError(f"scene schema must be {SCENE_SCHEMA}")
        if not self.simulation_frame.startswith(f"{self.frame_revision.uri}/frame/"):
            raise ValueError("simulation frame must belong to the frame revision")
        prototype_ids = [prototype.prototype_id for prototype in self.prototypes]
        if len(set(prototype_ids)) != len(prototype_ids):
            raise ValueError("prototype identities must be unique")
        entity_ids = [entity.entity_id for entity in self.entities]
        if len(set(entity_ids)) != len(entity_ids):
            raise ValueError("entity identities must be unique")
        known_prototypes = set(prototype_ids)
        if any(
            entity.prototype_id not in known_prototypes for entity in self.entities
        ):
            raise ValueError("every entity must reference a declared prototype")
        if len(set(self.allowed_camera_kinds)) != len(self.allowed_camera_kinds):
            raise ValueError("allowed camera kinds must be unique")
        total_bytes = self.environment.byte_length + sum(
            prototype.asset.byte_length for prototype in self.prototypes
        )
        if total_bytes > self.quality.maximum_asset_bytes:
            raise ValueError("scene artifacts exceed the declared byte limit")
        return self

    def canonical_bytes(self) -> bytes:
        """Return the exact compact field-ordered JSON hashed by Rust."""

        value = self.model_dump(mode="json", by_alias=True, exclude_none=True)
        return json.dumps(
            value,
            ensure_ascii=False,
            allow_nan=False,
            separators=(",", ":"),
        ).encode()


class SceneDeclaration(ContractModel):
    body: SceneDeclarationBody
    digest: Sha256Digest

    @classmethod
    def from_body(cls, body: SceneDeclarationBody) -> Self:
        digest = hashlib.sha256(body.canonical_bytes()).hexdigest()
        return cls(body=body, digest=f"sha256:{digest}")

    @model_validator(mode="after")
    def digest_matches(self) -> Self:
        expected = f"sha256:{hashlib.sha256(self.body.canonical_bytes()).hexdigest()}"
        if self.digest != expected:
            raise ValueError("scene digest does not match the canonical body")
        return self

    def wire(self) -> dict[str, object]:
        return self.model_dump(mode="json", by_alias=True, exclude_none=True)


IDENTITY_TRANSFORM = LocalTransform(
    translation_m=Vector3(x=0.0, y=0.0, z=0.0),
    orientation_xyzw=QuaternionXyzw(x=0.0, y=0.0, z=0.0, w=1.0),
    scale=Vector3(x=1.0, y=1.0, z=1.0),
)
