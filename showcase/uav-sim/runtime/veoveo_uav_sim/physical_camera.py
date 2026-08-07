from __future__ import annotations

from dataclasses import dataclass
from typing import Any

from .operator_camera import Pose, compose_pose


PHYSICAL_CAMERA_ROOT = "/World/PhysicalCameras"
PHYSICAL_VERTICAL_APERTURE_MM = 15.2908


def physical_camera_path(vehicle_id: str) -> str:
    if not vehicle_id or not all(
        character.isascii()
        and (character.isalnum() or character in {"_", "-"})
        for character in vehicle_id
    ):
        raise ValueError("physical RGB camera vehicle identity is invalid")
    return f"{PHYSICAL_CAMERA_ROOT}/{vehicle_id.replace('-', '_')}_down"


def physical_camera_product_name(vehicle_id: str) -> str:
    return f"physical_{physical_camera_path(vehicle_id).rsplit('/', 1)[-1]}"


@dataclass(slots=True)
class PhysicalRgbCamera:
    """One physical USD camera whose Hydra product provides RGB evidence."""

    path: str
    mount_pose: Pose
    _transform_operation: Any
    last_pose: Pose | None = None

    def update(self, vehicle_pose: Pose) -> Pose:
        from pxr import Gf

        pose = compose_pose(vehicle_pose, self.mount_pose)
        orientation = pose.orientation_xyzw.normalized()
        matrix = Gf.Matrix4d().SetRotate(
            Gf.Quatd(
                orientation.w,
                Gf.Vec3d(orientation.x, orientation.y, orientation.z),
            )
        )
        matrix.SetTranslateOnly(Gf.Vec3d(*pose.position_m.as_tuple()))
        self._transform_operation.Set(matrix)
        self.last_pose = pose
        return pose


def create_physical_rgb_camera(
    stage: Any,
    *,
    path: str,
    mount_pose: Pose,
    focal_length_mm: float,
    width_px: int,
    height_px: int,
    clipping_near_m: float,
    clipping_far_m: float,
) -> PhysicalRgbCamera:
    """Author the physical mount without a second sensor scheduling schema."""

    from pxr import Gf, UsdGeom

    if not path.startswith("/World/"):
        raise ValueError("physical RGB camera path must be rooted below /World")
    if clipping_near_m <= 0.0 or clipping_far_m <= clipping_near_m:
        raise ValueError("physical RGB camera clipping range is invalid")
    if width_px < 1 or height_px < 1:
        raise ValueError("physical RGB camera resolution is invalid")

    UsdGeom.Xform.Define(stage, PHYSICAL_CAMERA_ROOT)
    camera = UsdGeom.Camera.Define(stage, path)
    camera.CreateVerticalApertureAttr(PHYSICAL_VERTICAL_APERTURE_MM)
    camera.CreateHorizontalApertureAttr(
        PHYSICAL_VERTICAL_APERTURE_MM * (width_px / height_px)
    )
    camera.CreateFocalLengthAttr(float(focal_length_mm))
    camera.CreateClippingRangeAttr(
        Gf.Vec2f(float(clipping_near_m), float(clipping_far_m))
    )
    xformable = UsdGeom.Xformable(camera.GetPrim())
    xformable.ClearXformOpOrder()
    transform = xformable.AddTransformOp(
        precision=UsdGeom.XformOp.PrecisionDouble
    )
    return PhysicalRgbCamera(
        path=path,
        mount_pose=mount_pose.normalized(),
        _transform_operation=transform,
    )
