from __future__ import annotations

import json
import os
import re
import time
from collections.abc import Mapping
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any
from urllib.parse import urlsplit

from .contracts import ContractError, identity, object_with_keys

CATALOG_SCHEMA = "veoveo.io/simulation-view-layer-catalog/v1"
ENVIRONMENT = re.compile(r"^SIMULATION_VIEW_LAYER_[A-Z0-9_]{1,106}$")


@dataclass(frozen=True, slots=True)
class LayerDefinition:
    layer_id: str
    source: dict[str, Any]
    allowed_hosts: frozenset[str]
    allowed_redirect_hosts: frozenset[str]
    budgets: dict[str, int | float]
    license: dict[str, Any]
    georeference: dict[str, Any]
    credential: str | None = field(repr=False)


class LayerCatalog:
    def __init__(self, layers: Mapping[str, LayerDefinition]) -> None:
        self._layers = dict(layers)

    @classmethod
    def load(
        cls, path: Path, environment: Mapping[str, str] | None = None
    ) -> LayerCatalog:
        if not path.is_absolute() or not path.is_file() or path.is_symlink():
            raise ValueError(
                "SIMULATION_VIEW_LAYER_CATALOG must be a mounted regular file"
            )
        try:
            value = json.loads(path.read_bytes())
        except (OSError, json.JSONDecodeError) as error:
            raise ValueError("Simulation View layer catalog is invalid JSON") from error
        document = object_with_keys(
            "layer catalog", value, {"schemaVersion", "layers"}
        )
        if document["schemaVersion"] != CATALOG_SCHEMA:
            raise ValueError("Simulation View layer catalog schema is unsupported")
        if not isinstance(document["layers"], list):
            raise ContractError(
                "Simulation View layer catalog layers must be an array"
            )
        process_environment = os.environ if environment is None else environment
        layers: dict[str, LayerDefinition] = {}
        for raw in document["layers"]:
            layer = _parse_layer(raw, process_environment)
            if layer.layer_id in layers:
                raise ValueError(
                    f"duplicate Simulation View layer {layer.layer_id!r}"
                )
            layers[layer.layer_id] = layer
        return cls(layers)

    def get(self, layer_id: str) -> LayerDefinition:
        try:
            return self._layers[layer_id]
        except KeyError as error:
            raise ContractError(
                f"geospatial layer {layer_id!r} is not configured"
            ) from error


def _parse_layer(
    value: object, environment: Mapping[str, str]
) -> LayerDefinition:
    body = object_with_keys(
        "geospatial layer",
        value,
        {
            "layerId",
            "layerType",
            "source",
            "allowedHosts",
            "allowedRedirectHosts",
            "budgets",
            "license",
            "georeference",
        },
    )
    layer_id = identity("layerId", body["layerId"])
    if body["layerType"] != "streamed_3d_tiles":
        raise ValueError(f"geospatial layer {layer_id!r} has an unsupported type")
    allowed_hosts = _hosts(
        "allowedHosts", body["allowedHosts"], require_nonempty=True
    )
    redirect_hosts = _hosts(
        "allowedRedirectHosts",
        body["allowedRedirectHosts"],
        require_nonempty=False,
    )
    source = _source(layer_id, body["source"], allowed_hosts)
    credential_environment = source.get("credentialEnvironment")
    credential: str | None = None
    if credential_environment is not None:
        if (
            not isinstance(credential_environment, str)
            or ENVIRONMENT.fullmatch(credential_environment) is None
        ):
            raise ValueError(
                f"geospatial layer {layer_id!r} has an invalid credential environment"
            )
        credential = environment.get(credential_environment, "")
        if not credential or any(character.isspace() for character in credential):
            raise ValueError(
                f"geospatial layer {layer_id!r} requires Secret-backed environment "
                f"{credential_environment}"
            )
    budgets = object_with_keys(
        "layer budgets",
        body["budgets"],
        {
            "maximumCacheBytes",
            "maximumTileBytes",
            "maximumVisibleTiles",
            "maximumPendingTiles",
            "maximumScreenSpaceError",
        },
    )
    _validate_budgets(layer_id, budgets)
    license_value = object_with_keys(
        "layer license",
        body["license"],
        {"identifier", "attribution", "attributionUrl", "displayRequired"},
    )
    if (
        not isinstance(license_value["identifier"], str)
        or not license_value["identifier"].strip()
        or not isinstance(license_value["attribution"], str)
        or not license_value["attribution"].strip()
        or license_value["displayRequired"] is not True
    ):
        raise ValueError(f"geospatial layer {layer_id!r} has invalid attribution")
    _https_url("attributionUrl", license_value["attributionUrl"])
    georeference = object_with_keys(
        "layer georeference",
        body["georeference"],
        {"world", "frameRevision", "localEnuFrame", "origin"},
    )
    _validate_georeference(layer_id, georeference)
    return LayerDefinition(
        layer_id=layer_id,
        source=source,
        allowed_hosts=allowed_hosts,
        allowed_redirect_hosts=redirect_hosts,
        budgets=budgets,
        license=license_value,
        georeference=georeference,
        credential=credential,
    )


def _source(
    layer_id: str, value: object, allowed_hosts: frozenset[str]
) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise ContractError(
            f"geospatial layer {layer_id!r} source must be an object"
        )
    if value.get("kind") == "cesium_ion":
        source = object_with_keys(
            "ion source",
            value,
            {
                "kind",
                "assetId",
                "serverUrl",
                "apiUrl",
                "applicationId",
                "credentialEnvironment",
            },
        )
        for name in ("assetId", "applicationId"):
            if (
                not isinstance(source[name], int)
                or isinstance(source[name], bool)
                or source[name] < 1
            ):
                raise ValueError(
                    f"geospatial layer {layer_id!r} has an invalid {name}"
                )
        urls = (source["serverUrl"], source["apiUrl"])
    elif value.get("kind") == "https_3d_tiles":
        source = object_with_keys(
            "HTTPS 3D Tiles source", value, {"kind", "rootUrl"}
        )
        urls = (source["rootUrl"],)
    else:
        raise ValueError(f"geospatial layer {layer_id!r} source is unsupported")
    for raw_url in urls:
        parsed = _https_url("source URL", raw_url)
        if parsed.hostname not in allowed_hosts:
            raise ValueError(
                f"geospatial layer source host {parsed.hostname!r} is denied"
            )
    return source


def _hosts(
    label: str, value: object, *, require_nonempty: bool
) -> frozenset[str]:
    if not isinstance(value, list) or (require_nonempty and not value):
        requirement = "a non-empty array" if require_nonempty else "an array"
        raise ValueError(f"{label} must be {requirement}")
    hosts: set[str] = set()
    for host in value:
        if (
            not isinstance(host, str)
            or not host
            or len(host) > 253
            or any(character.isspace() for character in host)
            or any(character in host for character in "/:@")
        ):
            raise ValueError(f"{label} contains an invalid host")
        hosts.add(host.lower())
    if len(hosts) != len(value):
        raise ValueError(f"{label} contains a duplicate host")
    return frozenset(hosts)


def _https_url(label: str, value: object):
    if not isinstance(value, str):
        raise ContractError(f"{label} must be an HTTPS URL")
    parsed = urlsplit(value)
    if (
        parsed.scheme != "https"
        or not parsed.hostname
        or parsed.username is not None
        or parsed.password is not None
        or parsed.fragment
    ):
        raise ValueError(f"{label} must be a credential-free HTTPS URL")
    return parsed


def _validate_budgets(layer_id: str, budgets: dict[str, Any]) -> None:
    integer_names = (
        "maximumCacheBytes",
        "maximumTileBytes",
        "maximumVisibleTiles",
        "maximumPendingTiles",
    )
    if any(
        not isinstance(budgets[name], int)
        or isinstance(budgets[name], bool)
        or budgets[name] < 1
        for name in integer_names
    ):
        raise ValueError(f"geospatial layer {layer_id!r} has invalid budgets")
    error = budgets["maximumScreenSpaceError"]
    if (
        not isinstance(error, (int, float))
        or isinstance(error, bool)
        or not 0.0 < float(error) <= 256.0
        or budgets["maximumTileBytes"] > budgets["maximumCacheBytes"]
    ):
        raise ValueError(f"geospatial layer {layer_id!r} has invalid budgets")


def _validate_georeference(
    layer_id: str, georeference: dict[str, Any]
) -> None:
    frame = object_with_keys(
        "layer frame revision", georeference["frameRevision"], {"uri", "digest"}
    )
    origin = object_with_keys(
        "layer WGS84 origin",
        georeference["origin"],
        {"latitudeDegrees", "longitudeDegrees", "ellipsoidHeightM"},
    )
    revision = frame["uri"]
    if (
        not isinstance(revision, str)
        or not revision.startswith("frames://world/")
        or georeference["world"] != revision
        or not isinstance(georeference["localEnuFrame"], str)
        or not georeference["localEnuFrame"].startswith(f"{revision}/frame/")
        or not isinstance(frame["digest"], str)
        or not re.fullmatch(r"sha256:[0-9a-f]{64}", frame["digest"])
    ):
        raise ValueError(
            f"geospatial layer {layer_id!r} has an invalid frame binding"
        )
    latitude = origin["latitudeDegrees"]
    longitude = origin["longitudeDegrees"]
    height = origin["ellipsoidHeightM"]
    if (
        not isinstance(latitude, (int, float))
        or isinstance(latitude, bool)
        or not -90.0 <= float(latitude) <= 90.0
        or not isinstance(longitude, (int, float))
        or isinstance(longitude, bool)
        or not -180.0 <= float(longitude) <= 180.0
        or not isinstance(height, (int, float))
        or isinstance(height, bool)
    ):
        raise ValueError(
            f"geospatial layer {layer_id!r} has an invalid WGS84 origin"
        )


class StreamedWorldManager:
    def __init__(self, stage: Any, catalog: LayerCatalog) -> None:
        from cesium.omniverse.bindings import (
            acquire_cesium_omniverse_interface,
        )

        self._stage = stage
        self._catalog = catalog
        self._interface = acquire_cesium_omniverse_interface()
        self._sessions: dict[str, dict[str, Any]] = {}
        self._camera_index = 0

    def bind(self, scene: Any) -> dict[str, object] | None:
        layer_id = scene.layer_id
        if layer_id is None:
            return None
        layer = self._catalog.get(layer_id)
        frame = layer.georeference["frameRevision"]
        if (
            frame["uri"] != scene.frame_uri
            or frame["digest"] != scene.frame_digest
            or layer.georeference["localEnuFrame"]
            != scene.declaration["body"]["simulationFrame"]
        ):
            raise ContractError(
                f"geospatial layer {layer_id!r} does not match the scene Frames revision"
            )
        health = self._author(scene.session_id, layer)
        self._sessions[scene.session_id] = health
        return self.status(scene.session_id)

    def tick(self, camera_paths: tuple[str, ...]) -> None:
        if not self._sessions:
            return
        from cesium.omniverse.bindings import Viewport as CesiumViewport
        from cesium.usd.plugins.CesiumUsdSchemas import Tileset
        from omni.kit.viewport.utility import get_active_viewport

        viewport = get_active_viewport()
        if viewport is None:
            self._fail_all("provider_unavailable", "active renderer viewport is unavailable")
            return
        if camera_paths:
            camera_path = camera_paths[self._camera_index % len(camera_paths)]
            self._camera_index += 1
            viewport.set_active_camera(camera_path)
        cesium_viewport = CesiumViewport()
        cesium_viewport.viewMatrix = viewport.view
        cesium_viewport.projMatrix = viewport.projection
        cesium_viewport.width = float(viewport.resolution[0])
        cesium_viewport.height = float(viewport.resolution[1])
        try:
            self._interface.on_update_frame([cesium_viewport], False)
            statistics = self._interface.get_render_statistics()
        except Exception:  # noqa: BLE001 - provider faults become typed health
            self._fail_all("provider_unavailable", "streamed-world provider update failed")
            return
        resident_bytes = int(statistics.tileset_cached_bytes)
        visible_tiles = int(statistics.tiles_rendered)
        pending_tiles = int(statistics.tiles_loading_worker) + int(
            statistics.tiles_loading_main
        )
        for runtime in self._sessions.values():
            budgets = runtime["layer"].budgets
            exceeded = (
                resident_bytes > budgets["maximumCacheBytes"]
                or visible_tiles > budgets["maximumVisibleTiles"]
                or pending_tiles > budgets["maximumPendingTiles"]
            )
            runtime.update(
                residentBytes=resident_bytes,
                visibleTileCount=visible_tiles,
                pendingTileCount=pending_tiles,
            )
            if exceeded:
                Tileset.Get(self._stage, runtime["tilesetPath"]).GetSuspendUpdateAttr().Set(True)
                runtime.update(
                    lifecycle="failed",
                    failure={
                        "code": "budget_exceeded",
                        "message": "streamed-world tile residency exceeded an installation budget",
                    },
                )
            elif visible_tiles > 0:
                runtime.update(lifecycle="ready", failure=None)
            elif time.monotonic() - runtime["startedAt"] > 120.0:
                Tileset.Get(
                    self._stage, runtime["tilesetPath"]
                ).GetSuspendUpdateAttr().Set(True)
                runtime.update(
                    lifecycle="failed",
                    failure={
                        "code": "unavailable_coverage",
                        "message": "streamed-world coverage did not become visible",
                    },
                )
            else:
                runtime.update(lifecycle="loading", failure=None)

    def status(self, session_id: str) -> dict[str, object] | None:
        runtime = self._sessions.get(session_id)
        if runtime is None:
            return None
        layer: LayerDefinition = runtime["layer"]
        return {
            "layerId": layer.layer_id,
            "lifecycle": runtime["lifecycle"],
            "residentBytes": runtime["residentBytes"],
            "visibleTileCount": runtime["visibleTileCount"],
            "pendingTileCount": runtime["pendingTileCount"],
            "attribution": layer.license["attribution"],
            "attributionUrl": layer.license["attributionUrl"],
            "failure": runtime["failure"],
        }

    def ready(self) -> bool:
        return all(
            runtime["lifecycle"] != "failed"
            for runtime in self._sessions.values()
        )

    def close(self, session_id: str) -> None:
        from cesium.usd.plugins.CesiumUsdSchemas import Tileset
        from pxr import Usd

        runtime = self._sessions.pop(session_id, None)
        if runtime is None:
            return
        previous = self._stage.GetEditTarget()
        self._stage.SetEditTarget(Usd.EditTarget(self._stage.GetSessionLayer()))
        try:
            tileset = Tileset.Get(self._stage, runtime["tilesetPath"])
            if tileset.GetPrim().IsValid():
                tileset.GetIonAccessTokenAttr().Clear()
            self._stage.RemovePrim(runtime["rootPath"])
        finally:
            self._stage.SetEditTarget(previous)

    def close_all(self) -> None:
        for session_id in tuple(self._sessions):
            self.close(session_id)

    def _author(
        self, session_id: str, layer: LayerDefinition
    ) -> dict[str, Any]:
        from cesium.usd.plugins.CesiumUsdSchemas import (
            Georeference,
            IonServer,
            Tileset,
            Tokens,
        )
        from pxr import Usd

        root = f"/World/SimulationView/StreamedWorld/{_prim_name(session_id)}"
        previous = self._stage.GetEditTarget()
        self._stage.SetEditTarget(Usd.EditTarget(self._stage.GetSessionLayer()))
        try:
            self._stage.DefinePrim(root, "Scope")
            georeference_path = f"{root}/Georeference"
            georeference = Georeference.Define(self._stage, georeference_path)
            origin = layer.georeference["origin"]
            georeference.GetGeoreferenceOriginLatitudeAttr().Set(
                float(origin["latitudeDegrees"])
            )
            georeference.GetGeoreferenceOriginLongitudeAttr().Set(
                float(origin["longitudeDegrees"])
            )
            georeference.GetGeoreferenceOriginHeightAttr().Set(
                float(origin["ellipsoidHeightM"])
            )
            tileset_path = f"{root}/Tileset"
            tileset = Tileset.Define(self._stage, tileset_path)
            tileset.GetGeoreferenceBindingRel().SetTargets([georeference_path])
            source = layer.source
            if source["kind"] == "cesium_ion":
                server_path = f"{root}/IonServer"
                server = IonServer.Define(self._stage, server_path)
                server.GetDisplayNameAttr().Set(layer.layer_id)
                server.GetIonServerUrlAttr().Set(source["serverUrl"])
                server.GetIonServerApiUrlAttr().Set(source["apiUrl"])
                server.GetIonServerApplicationIdAttr().Set(source["applicationId"])
                tileset.GetSourceTypeAttr().Set(Tokens.ion)
                tileset.GetIonAssetIdAttr().Set(source["assetId"])
                tileset.GetIonAccessTokenAttr().Set(layer.credential)
                tileset.GetIonServerBindingRel().SetTargets([server_path])
            else:
                tileset.GetSourceTypeAttr().Set(Tokens.url)
                tileset.GetUrlAttr().Set(source["rootUrl"])
            budgets = layer.budgets
            tileset.GetMaximumCachedBytesAttr().Set(
                budgets["maximumCacheBytes"]
            )
            tileset.GetMaximumScreenSpaceErrorAttr().Set(
                float(budgets["maximumScreenSpaceError"])
            )
            tileset.GetMaximumSimultaneousTileLoadsAttr().Set(
                budgets["maximumPendingTiles"]
            )
            tileset.GetShowCreditsOnScreenAttr().Set(True)
        finally:
            self._stage.SetEditTarget(previous)
        self._interface.on_stage_change(0)
        import omni.usd

        self._interface.on_stage_change(omni.usd.get_context().get_stage_id())
        return {
            "layer": layer,
            "rootPath": root,
            "tilesetPath": tileset_path,
            "lifecycle": "loading",
            "residentBytes": 0,
            "visibleTileCount": 0,
            "pendingTileCount": 0,
            "failure": None,
            "startedAt": time.monotonic(),
        }

    def _fail_all(self, code: str, message: str) -> None:
        for runtime in self._sessions.values():
            runtime.update(
                lifecycle="failed",
                failure={"code": code, "message": message},
            )


def _prim_name(value: str) -> str:
    return "v_" + re.sub(r"[^A-Za-z0-9_]", "_", value)
