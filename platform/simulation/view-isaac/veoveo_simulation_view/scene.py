from __future__ import annotations

import hashlib
import json
import os
import re
import secrets
import shutil
import struct
import threading
import zipfile
from dataclasses import dataclass
from pathlib import Path, PurePosixPath
from typing import Any, BinaryIO

from .contracts import (
    ContractError,
    SceneBinding,
    identity,
    object_with_keys,
)
from .lighting import (
    DiagnosticScene,
    author_governed_lighting,
)
from .pose import PoseSnapshot


FORBIDDEN_CONTENT = (
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
)
USD_ASSET_REFERENCE = re.compile(rb"@([^@]+)@")
FORMAT_SUFFIX = {
    "usd": ".usd",
    "usdz": ".usdz",
    "glb": ".glb",
    "gltf": ".gltf",
}


@dataclass(frozen=True, slots=True)
class ResolvedArtifact:
    path: Path
    digest: str
    format: str


class ArtifactMaterializer:
    def __init__(self, directory: Path, maximum_bytes: int) -> None:
        self._directory = directory.resolve()
        self._sha_directory = self._directory / "sha256"
        self._maximum_bytes = maximum_bytes
        self._lock = threading.Lock()
        if (
            not self._sha_directory.is_dir()
            or self._sha_directory.is_symlink()
            or not self._sha_directory.resolve().is_relative_to(self._directory)
        ):
            raise ValueError(
                "renderer artifact sha256 directory must be materialized safely"
            )

    def materialize(
        self,
        hexadecimal: str,
        asset_format: str,
        byte_length: int,
        source: BinaryIO,
    ) -> Path:
        if (
            len(hexadecimal) != 64
            or any(character not in "0123456789abcdef" for character in hexadecimal)
            or asset_format not in FORMAT_SUFFIX
            or not 1 <= byte_length <= self._maximum_bytes
        ):
            raise ContractError("artifact materialization declaration is invalid")
        destination = (
            self._sha_directory
            / f"{hexadecimal}{FORMAT_SUFFIX[asset_format]}"
        )
        with self._lock:
            if destination.exists() or destination.is_symlink():
                self._verify_existing(destination, hexadecimal, byte_length)
                return destination
            temporary = self._sha_directory / (
                f".{hexadecimal}.{secrets.token_hex(12)}.next"
            )
            flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL
            if hasattr(os, "O_NOFOLLOW"):
                flags |= os.O_NOFOLLOW
            descriptor = os.open(temporary, flags, 0o600)
            try:
                digest = hashlib.sha256()
                remaining = byte_length
                with os.fdopen(descriptor, "wb", closefd=True) as output:
                    descriptor = -1
                    while remaining:
                        chunk = source.read(min(1024 * 1024, remaining))
                        if not chunk:
                            raise ContractError(
                                "artifact upload ended before its declared byte length"
                            )
                        output.write(chunk)
                        digest.update(chunk)
                        remaining -= len(chunk)
                    output.flush()
                    os.fsync(output.fileno())
                if digest.hexdigest() != hexadecimal:
                    raise ContractError(
                        "artifact upload digest does not match its declaration"
                    )
                os.replace(temporary, destination)
                destination.chmod(0o600)
                return destination
            finally:
                if descriptor >= 0:
                    os.close(descriptor)
                temporary.unlink(missing_ok=True)

    def _verify_existing(
        self, path: Path, hexadecimal: str, byte_length: int
    ) -> None:
        if path.is_symlink() or not path.is_file():
            raise ContractError(
                "materialized artifact path is not a regular file"
            )
        resolved = path.resolve()
        if not resolved.is_relative_to(self._directory):
            raise ContractError(
                "materialized artifact escaped its materialization root"
            )
        if resolved.stat().st_size != byte_length:
            raise ContractError(
                "materialized artifact byte length does not match"
            )
        digest = hashlib.sha256()
        with resolved.open("rb") as existing:
            while chunk := existing.read(1024 * 1024):
                digest.update(chunk)
        if digest.hexdigest() != hexadecimal:
            raise ContractError("materialized artifact digest does not match")


class ArtifactStore:
    def __init__(self, directory: Path, cache_directory: Path) -> None:
        self._directory = directory.resolve()
        self._cache = (cache_directory / "assets").resolve()
        self._cache.mkdir(parents=True, exist_ok=True)

    def resolve(self, value: object) -> ResolvedArtifact:
        artifact = object_with_keys(
            "governed artifact",
            value,
            {"artifactUri", "digest", "format", "byteLength"},
        )
        uri = artifact["artifactUri"]
        digest = artifact["digest"]
        asset_format = artifact["format"]
        byte_length = artifact["byteLength"]
        if (
            not isinstance(uri, str)
            or not uri.startswith("artifact://")
            or not isinstance(digest, str)
            or not digest.startswith("sha256:")
            or len(digest) != 71
            or asset_format not in FORMAT_SUFFIX
            or not isinstance(byte_length, int)
            or isinstance(byte_length, bool)
            or byte_length < 1
        ):
            raise ContractError("governed artifact declaration is invalid")
        hexadecimal = digest[7:]
        if any(character not in "0123456789abcdef" for character in hexadecimal):
            raise ContractError("governed artifact digest is invalid")
        path = (
            self._directory
            / "sha256"
            / f"{hexadecimal}{FORMAT_SUFFIX[asset_format]}"
        )
        if path.is_symlink() or not path.is_file():
            raise ContractError("governed artifact is not materialized")
        resolved = path.resolve()
        if not resolved.is_relative_to(self._directory):
            raise ContractError("governed artifact escaped its materialization root")
        data = resolved.read_bytes()
        if len(data) != byte_length:
            raise ContractError("governed artifact byte length does not match")
        if hashlib.sha256(data).hexdigest() != hexadecimal:
            raise ContractError("governed artifact digest does not match")
        load_path = self._preflight(asset_format, hexadecimal, data, resolved)
        return ResolvedArtifact(
            path=load_path, digest=digest, format=asset_format
        )

    def _preflight(
        self,
        asset_format: str,
        hexadecimal: str,
        data: bytes,
        source: Path,
    ) -> Path:
        if asset_format == "usd":
            _validate_usda(data, allow_relative_assets=False)
            return source
        if asset_format == "usdz":
            return self._extract_usdz(hexadecimal, source)
        if asset_format == "glb":
            _validate_glb(data)
            return source
        if asset_format == "gltf":
            try:
                value = json.loads(data)
            except (UnicodeDecodeError, json.JSONDecodeError) as error:
                raise ContractError("glTF JSON is invalid") from error
            _validate_gltf(value)
            return source
        raise AssertionError("validated format was not handled")

    def _extract_usdz(self, hexadecimal: str, source: Path) -> Path:
        destination = self._cache / hexadecimal
        marker = destination / ".verified"
        if marker.is_file():
            root = _usdz_root(destination)
            return root
        temporary = self._cache / f"{hexadecimal}.next"
        if temporary.exists():
            shutil.rmtree(temporary)
        temporary.mkdir(mode=0o700)
        total_bytes = 0
        with zipfile.ZipFile(source) as archive:
            names = archive.namelist()
            if not names:
                raise ContractError("USDZ archive is empty")
            for member in archive.infolist():
                relative = PurePosixPath(member.filename)
                if (
                    relative.is_absolute()
                    or ".." in relative.parts
                    or "\\" in member.filename
                    or not relative.name
                    or member.is_dir()
                    or member.file_size < 1
                    or relative.suffix.lower()
                    in {".py", ".so", ".dll", ".exe", ".sh"}
                ):
                    raise ContractError("USDZ archive contains an unsafe member")
                total_bytes += member.file_size
                if total_bytes > 4 * 1024 * 1024 * 1024:
                    raise ContractError("USDZ expanded size exceeds the limit")
                payload = archive.read(member)
                if relative.suffix.lower() in {".usd", ".usda"}:
                    _validate_usda(
                        payload,
                        allow_relative_assets=True,
                        archive_members=set(names),
                    )
                output = temporary.joinpath(*relative.parts)
                output.parent.mkdir(parents=True, exist_ok=True)
                output.write_bytes(payload)
                output.chmod(0o600)
        (temporary / ".verified").write_text(
            f"sha256:{hexadecimal}\n", encoding="utf-8"
        )
        if destination.exists():
            shutil.rmtree(destination)
        temporary.rename(destination)
        return _usdz_root(destination)


class SceneManager:
    def __init__(
        self,
        stage: Any,
        artifacts: ArtifactStore,
        diagnostics: DiagnosticScene,
    ) -> None:
        self._stage = stage
        self._artifacts = artifacts
        self._diagnostics = diagnostics
        self._scenes: dict[str, SceneBinding] = {}
        self._entities: dict[tuple[str, str], tuple[Any, Any]] = {}

    def bind(self, scene: SceneBinding) -> None:
        from pxr import UsdGeom

        existing = self._scenes.get(scene.session_id)
        if existing is not None:
            if existing.declaration["digest"] == scene.declaration["digest"]:
                return
            raise ContractError("renderer scene is immutable")
        body = scene.declaration["body"]
        environment = self._artifacts.resolve(body["environment"])
        prototypes: dict[str, tuple[ResolvedArtifact, dict[str, Any]]] = {}
        for prototype in body["prototypes"]:
            value = object_with_keys(
                "visual prototype",
                prototype,
                {"prototypeId", "asset", "localAlignment"},
            )
            prototype_id = identity("prototypeId", value["prototypeId"])
            if prototype_id in prototypes:
                raise ContractError("visual prototype identity is duplicated")
            prototypes[prototype_id] = (
                self._artifacts.resolve(value["asset"]),
                value["localAlignment"],
            )

        root = _session_root(scene.session_id)
        self._stage.DefinePrim(root, "Xform")
        environment_prim = self._stage.DefinePrim(
            f"{root}/Environment", "Xform"
        )
        environment_prim.GetReferences().AddReference(str(environment.path))

        prototype_root = f"{root}/Prototypes"
        self._stage.DefinePrim(prototype_root, "Scope")
        for prototype_id, (artifact, alignment) in prototypes.items():
            path = f"{prototype_root}/{_prim_name(prototype_id)}"
            prim = self._stage.DefinePrim(path, "Xform")
            prim.GetReferences().AddReference(str(artifact.path))
            _set_transform(UsdGeom.Xformable(prim), alignment)

        entity_root = f"{root}/Entities"
        self._stage.DefinePrim(entity_root, "Scope")
        seen: set[str] = set()
        for entity in body["entities"]:
            value = object_with_keys(
                "scene entity",
                entity,
                {"entityId", "prototypeId"}
                if "staticTransform" not in entity
                else {"entityId", "prototypeId", "staticTransform"},
            )
            entity_id = identity("entityId", value["entityId"])
            prototype_id = identity("prototypeId", value["prototypeId"])
            if entity_id in seen or prototype_id not in prototypes:
                raise ContractError("scene entity binding is invalid")
            seen.add(entity_id)
            path = f"{entity_root}/{_prim_name(entity_id)}"
            prim = self._stage.DefinePrim(path, "Xform")
            prim.GetReferences().AddInternalReference(
                f"{prototype_root}/{_prim_name(prototype_id)}"
            )
            prim.SetInstanceable(True)
            xform = UsdGeom.Xformable(prim)
            operation = _set_transform(
                xform,
                value.get("staticTransform")
                or {
                    "translationM": {"x": 0.0, "y": 0.0, "z": 0.0},
                    "orientationXyzw": {
                        "x": 0.0,
                        "y": 0.0,
                        "z": 0.0,
                        "w": 1.0,
                    },
                    "scale": {"x": 1.0, "y": 1.0, "z": 1.0},
                },
            )
            visibility = UsdGeom.Imageable(prim).CreateVisibilityAttr(
                UsdGeom.Tokens.inherited
            )
            self._entities[(scene.session_id, entity_id)] = (
                operation,
                visibility,
            )

        author_governed_lighting(self._stage, root, scene.lighting)
        try:
            self._diagnostics.enter_governed_session()
        except BaseException:
            self._stage.RemovePrim(root)
            self._entities = {
                key: value
                for key, value in self._entities.items()
                if key[0] != scene.session_id
            }
            raise
        self._scenes[scene.session_id] = scene

    def apply_pose(self, snapshot: PoseSnapshot) -> None:
        from pxr import Gf, UsdGeom

        if snapshot.session_id not in self._scenes:
            raise ContractError("pose session has no bound renderer scene")
        for entity in snapshot.entities:
            binding = self._entities.get(
                (snapshot.session_id, entity.entity_id)
            )
            if binding is None:
                raise ContractError("pose entity is absent from the scene")
            operation, visibility = binding
            x, y, z, w = entity.orientation_xyzw
            transform = Gf.Transform()
            transform.SetRotation(
                Gf.Rotation(Gf.Quatd(w, Gf.Vec3d(x, y, z)))
            )
            transform.SetTranslation(Gf.Vec3d(*entity.position_enu_m))
            operation.Set(transform.GetMatrix())
            visibility.Set(
                UsdGeom.Tokens.inherited
                if entity.active and entity.visible
                else UsdGeom.Tokens.invisible
            )

    def close(self, session_id: str) -> None:
        self._stage.RemovePrim(_session_root(session_id))
        scene = self._scenes.pop(session_id, None)
        self._entities = {
            key: value
            for key, value in self._entities.items()
            if key[0] != session_id
        }
        if scene is not None:
            self._diagnostics.leave_governed_session()

    def mark_pose_stale(self, session_id: str) -> None:
        from pxr import UsdGeom

        for (entity_session, _), (_, visibility) in self._entities.items():
            if entity_session == session_id:
                visibility.Set(UsdGeom.Tokens.invisible)


def _validate_usda(
    data: bytes,
    *,
    allow_relative_assets: bool,
    archive_members: set[str] | None = None,
) -> None:
    if not data.startswith(b"#usda"):
        raise ContractError("only declarative USDA layers are supported")
    lower = data.lower()
    if any(token in lower for token in FORBIDDEN_CONTENT):
        raise ContractError("USD layer contains forbidden executable or network content")
    references = USD_ASSET_REFERENCE.findall(data)
    if references and not allow_relative_assets:
        raise ContractError("standalone USD must be self-contained")
    for reference in references:
        try:
            value = reference.decode("utf-8")
        except UnicodeDecodeError as error:
            raise ContractError("USD asset reference is invalid") from error
        path = PurePosixPath(value)
        if (
            path.is_absolute()
            or ".." in path.parts
            or archive_members is None
            or value not in archive_members
        ):
            raise ContractError("USDZ asset reference escapes the archive")


def _validate_glb(data: bytes) -> None:
    if len(data) < 20:
        raise ContractError("GLB is truncated")
    magic, version, declared = struct.unpack_from("<4sII", data)
    if magic != b"glTF" or version != 2 or declared != len(data):
        raise ContractError("GLB header is invalid")
    json_length, json_type = struct.unpack_from("<II", data, 12)
    if json_type != 0x4E4F534A or 20 + json_length > len(data):
        raise ContractError("GLB JSON chunk is invalid")
    try:
        value = json.loads(data[20 : 20 + json_length])
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ContractError("GLB JSON chunk is invalid") from error
    _validate_gltf(value)


def _validate_gltf(value: object) -> None:
    if not isinstance(value, dict) or value.get("asset", {}).get("version") != "2.0":
        raise ContractError("glTF 2.0 asset declaration is required")
    for collection in ("buffers", "images"):
        for item in value.get(collection, []):
            if not isinstance(item, dict):
                raise ContractError("glTF resource declaration is invalid")
            uri = item.get("uri")
            if uri is not None and (
                not isinstance(uri, str)
                or not uri.startswith("data:")
                or len(uri) > 64 * 1024 * 1024
            ):
                raise ContractError("glTF external resources are forbidden")
    if any(
        key.lower().startswith(("script", "physics"))
        for key in value
    ):
        raise ContractError("glTF executable or physics content is forbidden")
    _reject_gltf_content(value)


def _reject_gltf_content(value: object) -> None:
    if isinstance(value, dict):
        for key, child in value.items():
            lowered = str(key).lower()
            if "script" in lowered or "physics" in lowered:
                raise ContractError(
                    "glTF executable or physics content is forbidden"
                )
            _reject_gltf_content(child)
    elif isinstance(value, list):
        for child in value:
            _reject_gltf_content(child)
    elif isinstance(value, str):
        lowered = value.lower()
        if lowered.startswith(("http:", "https:", "file:", "omniverse:")):
            raise ContractError("glTF network resources are forbidden")


def _usdz_root(directory: Path) -> Path:
    roots = sorted(
        path
        for path in directory.rglob("*")
        if path.suffix.lower() in {".usd", ".usda"}
    )
    if not roots:
        raise ContractError("USDZ archive has no declarative root layer")
    return roots[0]


def _set_transform(xform: Any, value: dict[str, Any]) -> Any:
    from pxr import Gf, UsdGeom

    translation = value["translationM"]
    orientation = value["orientationXyzw"]
    scale = value["scale"]
    xform.ClearXformOpOrder()
    operation = xform.AddTransformOp(
        precision=UsdGeom.XformOp.PrecisionDouble
    )
    transform = Gf.Transform()
    transform.SetRotation(
        Gf.Rotation(
            Gf.Quatd(
                float(orientation["w"]),
                Gf.Vec3d(
                    float(orientation["x"]),
                    float(orientation["y"]),
                    float(orientation["z"]),
                ),
            )
        )
    )
    transform.SetScale(
        Gf.Vec3d(
            float(scale["x"]),
            float(scale["y"]),
            float(scale["z"]),
        )
    )
    transform.SetTranslation(
        Gf.Vec3d(
            float(translation["x"]),
            float(translation["y"]),
            float(translation["z"]),
        )
    )
    operation.Set(transform.GetMatrix())
    return operation


def _prim_name(value: str) -> str:
    return "v_" + re.sub(r"[^A-Za-z0-9_]", "_", value)


def _session_root(session_id: str) -> str:
    return f"/World/SimulationView/Sessions/{_prim_name(session_id)}"
