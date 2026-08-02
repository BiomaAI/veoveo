from __future__ import annotations

from dataclasses import dataclass
from enum import Enum
from pathlib import Path
from typing import Any


CESIUM_EXTENSION_DIRECTORY = Path(
    "/opt/veoveo/extensions/cesium.omniverse"
)
CESIUM_MDL_DIRECTORY = CESIUM_EXTENSION_DIRECTORY / "mdl"
CESIUM_MDL_MODULE = CESIUM_MDL_DIRECTORY / "cesium.mdl"

TANGENT_FRAME_SETTING = "/rtx/hydra/TBNFrameMode"
MATERIAL_SEARCH_PATH_SETTING = "materialConfig/searchPaths/custom"
MATERIAL_ALLOWLIST_SETTING = (
    "materialConfig/materialGraph/userAllowList"
)
RENDERER_MDL_SEARCH_PATH_SETTING = "/renderer/mdl/searchPaths/custom"
CESIUM_MDL_MODULE_NAME = "cesium.mdl"


class RendererFailureCode(str, Enum):
    REQUIRED_EXTENSION_MISSING = "required_extension_missing"
    CESIUM_MDL_ASSETS_MISSING = "cesium_mdl_assets_missing"
    CESIUM_MATERIAL_SEARCH_PATH_MISSING = (
        "cesium_material_search_path_missing"
    )
    CESIUM_MATERIAL_ALLOWLIST_MISSING = (
        "cesium_material_allowlist_missing"
    )
    CESIUM_TANGENT_FRAME_MISSING = "cesium_tangent_frame_missing"
    LDR_COLOR_PIPELINE_FAILED = "ldr_color_pipeline_failed"
    DIAGNOSTIC_LIGHT_ISOLATION_FAILED = (
        "diagnostic_light_isolation_failed"
    )
    RENDERER_INITIALIZATION_FAILED = "renderer_initialization_failed"


@dataclass(frozen=True, slots=True)
class RendererFailure:
    code: RendererFailureCode
    message: str

    def response(self) -> dict[str, str]:
        return {"code": self.code.value, "message": self.message}


class RendererInitializationError(RuntimeError):
    def __init__(self, failure: RendererFailure) -> None:
        super().__init__(failure.message)
        self.failure = failure


@dataclass(frozen=True, slots=True)
class CesiumMaterialStatus:
    mdl_assets_ready: bool
    material_search_path_ready: bool
    material_allowlist_ready: bool
    tangent_frame_ready: bool


def ensure_cesium_material_runtime(
    settings: Any,
    *,
    extension_directory: Path = CESIUM_EXTENSION_DIRECTORY,
) -> CesiumMaterialStatus:
    extension_directory = extension_directory.resolve()
    mdl_directory = extension_directory / "mdl"
    mdl_module = mdl_directory / CESIUM_MDL_MODULE_NAME
    if not mdl_directory.is_dir() or not mdl_module.is_file():
        raise RendererInitializationError(
            RendererFailure(
                RendererFailureCode.CESIUM_MDL_ASSETS_MISSING,
                "packaged Cesium MDL assets are unavailable",
            )
        )

    mdl_path = str(mdl_directory)
    settings.set(TANGENT_FRAME_SETTING, 1)
    settings.set_string_array(
        MATERIAL_SEARCH_PATH_SETTING,
        _append_unique(
            _string_array(settings, MATERIAL_SEARCH_PATH_SETTING),
            mdl_path,
        ),
    )
    settings.set_string_array(
        MATERIAL_ALLOWLIST_SETTING,
        _append_unique(
            _string_array(settings, MATERIAL_ALLOWLIST_SETTING),
            CESIUM_MDL_MODULE_NAME,
        ),
    )
    settings.set_string(
        RENDERER_MDL_SEARCH_PATH_SETTING,
        ";".join(
            _append_unique(
                _semicolon_paths(
                    settings.get_as_string(
                        RENDERER_MDL_SEARCH_PATH_SETTING
                    )
                ),
                mdl_path,
            )
        ),
    )

    status = CesiumMaterialStatus(
        mdl_assets_ready=mdl_directory.is_dir() and mdl_module.is_file(),
        material_search_path_ready=(
            _string_array(settings, MATERIAL_SEARCH_PATH_SETTING).count(
                mdl_path
            )
            == 1
            and _semicolon_paths(
                settings.get_as_string(
                    RENDERER_MDL_SEARCH_PATH_SETTING
                )
            ).count(mdl_path)
            == 1
        ),
        material_allowlist_ready=(
            _string_array(settings, MATERIAL_ALLOWLIST_SETTING).count(
                CESIUM_MDL_MODULE_NAME
            )
            == 1
        ),
        tangent_frame_ready=settings.get(TANGENT_FRAME_SETTING) == 1,
    )
    _verify_status(status)
    return status


def _verify_status(status: CesiumMaterialStatus) -> None:
    if not status.mdl_assets_ready:
        failure = RendererFailure(
            RendererFailureCode.CESIUM_MDL_ASSETS_MISSING,
            "packaged Cesium MDL assets are unavailable",
        )
    elif not status.material_search_path_ready:
        failure = RendererFailure(
            RendererFailureCode.CESIUM_MATERIAL_SEARCH_PATH_MISSING,
            "Cesium MDL search-path registration did not take effect",
        )
    elif not status.material_allowlist_ready:
        failure = RendererFailure(
            RendererFailureCode.CESIUM_MATERIAL_ALLOWLIST_MISSING,
            "Cesium MDL allowlist registration did not take effect",
        )
    elif not status.tangent_frame_ready:
        failure = RendererFailure(
            RendererFailureCode.CESIUM_TANGENT_FRAME_MISSING,
            "Cesium tangent-frame initialization did not take effect",
        )
    else:
        return
    raise RendererInitializationError(failure)


def _string_array(settings: Any, name: str) -> list[str]:
    value = settings.get(name)
    if value is None:
        return []
    if not isinstance(value, (list, tuple)) or any(
        not isinstance(item, str) or not item for item in value
    ):
        raise RendererInitializationError(
            RendererFailure(
                RendererFailureCode.RENDERER_INITIALIZATION_FAILED,
                "renderer material configuration has an invalid shape",
            )
        )
    return _unique(value)


def _semicolon_paths(value: str) -> list[str]:
    return _unique(
        item.strip() for item in value.split(";") if item.strip()
    )


def _append_unique(values: list[str], value: str) -> list[str]:
    return _unique([*values, value])


def _unique(values: Any) -> list[str]:
    result: list[str] = []
    for value in values:
        if value not in result:
            result.append(value)
    return result
