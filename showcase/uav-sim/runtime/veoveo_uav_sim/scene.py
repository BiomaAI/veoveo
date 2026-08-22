from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Any


@dataclass(frozen=True, slots=True)
class FleetScene:
    body_paths: tuple[str, ...]
    initial_positions_enu_m: tuple[tuple[float, float, float], ...]


def create_fleet_scene(stage: Any, asset_path: Path, vehicle_count: int) -> FleetScene:
    from isaacsim.core.experimental.objects import GroundPlane
    from pxr import Gf, Usd, UsdGeom

    if vehicle_count < 1:
        raise ValueError("UAV scene must contain at least one vehicle")
    if not asset_path.is_file():
        raise RuntimeError(f"repository UAV asset is missing: {asset_path}")

    UsdGeom.SetStageMetersPerUnit(stage, 1.0)
    UsdGeom.SetStageUpAxis(stage, UsdGeom.Tokens.z)
    ground = GroundPlane(
        "/World/uav_launch_surface",
        sizes=40.0,
        colors="darkslategray",
        templates=None,
    )
    for prim in ground.prims:
        for descendant in Usd.PrimRange(prim):
            if descendant.IsA(UsdGeom.Mesh):
                UsdGeom.Imageable(descendant).MakeInvisible()

    body_paths: list[str] = []
    initial_positions: list[tuple[float, float, float]] = []
    for index in range(vehicle_count):
        root_path = f"/World/uav_{index + 1}"
        root = stage.DefinePrim(root_path, "Xform")
        if not root.GetReferences().AddReference(str(asset_path)):
            raise RuntimeError(f"failed to reference UAV asset at {root_path}")
        position = (float(index * 3), 0.0, 0.18)
        xform = UsdGeom.Xformable(root)
        xform.ClearXformOpOrder()
        xform.AddTranslateOp().Set(Gf.Vec3d(*position))
        body_path = f"{root_path}/body"
        if not stage.GetPrimAtPath(body_path).IsValid():
            raise RuntimeError(f"UAV asset did not compose rigid body {body_path}")
        body_paths.append(body_path)
        initial_positions.append(position)

    return FleetScene(tuple(body_paths), tuple(initial_positions))
