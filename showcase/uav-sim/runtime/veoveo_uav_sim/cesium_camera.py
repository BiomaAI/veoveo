from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Protocol, TypeVar


@dataclass(frozen=True, slots=True)
class CesiumCameraSpec:
    camera_path: str
    width: int
    height: int

    def __post_init__(self) -> None:
        if not self.camera_path.startswith("/"):
            raise ValueError("Cesium camera path must be an absolute USD prim path")
        if self.width < 1 or self.height < 1:
            raise ValueError("Cesium camera dimensions must be positive")


CesiumViewportT = TypeVar("CesiumViewportT")


class CesiumViewportFactory(Protocol[CesiumViewportT]):
    def __call__(self) -> CesiumViewportT: ...


def current_cesium_viewport(
    stage: Any,
    spec: CesiumCameraSpec,
    viewport_type: CesiumViewportFactory[CesiumViewportT],
) -> CesiumViewportT:
    """Project the current USD camera into Cesium's native viewport contract."""
    from pxr import Usd, UsdGeom

    time_code = Usd.TimeCode.Default()
    usd_camera = UsdGeom.Camera.Get(stage, spec.camera_path)
    if not usd_camera.GetPrim().IsValid():
        raise RuntimeError(f"Cesium camera prim is unavailable: {spec.camera_path}")
    camera = usd_camera.GetCamera(time_code)
    camera.SetTransform(usd_camera.ComputeLocalToWorldTransform(time_code))
    frustum = camera.GetFrustum()
    viewport = viewport_type()
    viewport.viewMatrix = frustum.ComputeViewMatrix()
    viewport.projMatrix = frustum.ComputeProjectionMatrix()
    viewport.width = float(spec.width)
    viewport.height = float(spec.height)
    return viewport
