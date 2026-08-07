from __future__ import annotations

from typing import Any, Protocol, TypeVar

from .operator_camera import Pose


CesiumViewportT = TypeVar("CesiumViewportT")


class CesiumViewportFactory(Protocol[CesiumViewportT]):
    def __call__(self) -> CesiumViewportT: ...


def current_pose_cesium_viewport(
    stage: object,
    camera_path: str,
    pose: Pose,
    width: int,
    height: int,
    viewport_type: CesiumViewportFactory[CesiumViewportT],
) -> CesiumViewportT:
    """Project one authoritative camera pose into Cesium's viewport contract."""
    from pxr import Gf, Usd, UsdGeom

    time_code = Usd.TimeCode.Default()
    usd_camera = UsdGeom.Camera.Get(stage, camera_path)
    if not usd_camera.GetPrim().IsValid():
        raise RuntimeError(f"authoritative camera prim is unavailable: {camera_path}")
    orientation = pose.orientation_xyzw.normalized()
    rotation = Gf.Quatd(
        orientation.w,
        Gf.Vec3d(orientation.x, orientation.y, orientation.z),
    )
    transform = Gf.Matrix4d().SetRotate(rotation)
    transform.SetTranslateOnly(Gf.Vec3d(*pose.position_m.as_tuple()))
    frustum = camera_frustum_at_transform(usd_camera, time_code, transform)
    viewport = viewport_type()
    viewport.viewMatrix = frustum.ComputeViewMatrix()
    viewport.projMatrix = frustum.ComputeProjectionMatrix()
    viewport.width = float(width)
    viewport.height = float(height)
    return viewport


def camera_frustum_at_transform(
    usd_camera: Any, time_code: Any, transform: Any
) -> Any:
    """Return a camera frustum at an explicit authoritative world transform."""
    camera = usd_camera.GetCamera(time_code)
    camera.transform = transform
    return camera.frustum
