from __future__ import annotations

import math
from dataclasses import dataclass
from typing import Any

from .contracts import ContractError, finite_number, object_with_keys


MINIMUM_COLOR_TEMPERATURE_KELVIN = 1_000
MAXIMUM_COLOR_TEMPERATURE_KELVIN = 10_000
SUN_ANGULAR_DIAMETER_DEGREES = 0.53
SUN_ROTATION_DEGREES = (-45.0, -35.0, 0.0)
DIAGNOSTIC_SUN_INTENSITY = 3_000.0
DIAGNOSTIC_DOME_INTENSITY = 500.0

DIAGNOSTIC_ROOT = "/World/SimulationView/Diagnostics"
DIAGNOSTIC_SUN = f"{DIAGNOSTIC_ROOT}/Sun"
DIAGNOSTIC_DOME = f"{DIAGNOSTIC_ROOT}/Dome"


@dataclass(frozen=True, slots=True)
class GovernedLighting:
    sun_intensity_lux: float
    sky_intensity: float
    color_temperature_kelvin: int

    @classmethod
    def parse(cls, value: object) -> "GovernedLighting":
        body = object_with_keys(
            "scene lighting",
            value,
            {
                "sunIntensityLux",
                "skyIntensity",
                "colorTemperatureKelvin",
            },
        )
        sun_intensity = finite_number(
            "sunIntensityLux", body["sunIntensityLux"]
        )
        sky_intensity = finite_number(
            "skyIntensity", body["skyIntensity"]
        )
        temperature = body["colorTemperatureKelvin"]
        if sun_intensity <= 0.0:
            raise ContractError("sunIntensityLux must be positive")
        if sky_intensity <= 0.0:
            raise ContractError("skyIntensity must be positive")
        if (
            not isinstance(temperature, int)
            or isinstance(temperature, bool)
            or not MINIMUM_COLOR_TEMPERATURE_KELVIN
            <= temperature
            <= MAXIMUM_COLOR_TEMPERATURE_KELVIN
        ):
            raise ContractError(
                "colorTemperatureKelvin must be between 1000 and 10000"
            )
        return cls(
            sun_intensity_lux=sun_intensity,
            sky_intensity=sky_intensity,
            color_temperature_kelvin=temperature,
        )

    def openusd_settings(self) -> "OpenUsdLightingSettings":
        return OpenUsdLightingSettings(
            sun=OpenUsdDistantLightSettings(
                intensity=self.sun_intensity_lux,
                exposure=0.0,
                normalize=True,
                enable_color_temperature=True,
                color_temperature_kelvin=self.color_temperature_kelvin,
                angle_degrees=SUN_ANGULAR_DIAMETER_DEGREES,
                rotation_degrees=SUN_ROTATION_DEGREES,
            ),
            sky=OpenUsdDomeLightSettings(
                intensity=self.sky_intensity,
                exposure=0.0,
                normalize=False,
                enable_color_temperature=True,
                color_temperature_kelvin=self.color_temperature_kelvin,
            ),
        )


@dataclass(frozen=True, slots=True)
class OpenUsdDistantLightSettings:
    intensity: float
    exposure: float
    normalize: bool
    enable_color_temperature: bool
    color_temperature_kelvin: int
    angle_degrees: float
    rotation_degrees: tuple[float, float, float]


@dataclass(frozen=True, slots=True)
class OpenUsdDomeLightSettings:
    intensity: float
    exposure: float
    normalize: bool
    enable_color_temperature: bool
    color_temperature_kelvin: int


@dataclass(frozen=True, slots=True)
class OpenUsdLightingSettings:
    sun: OpenUsdDistantLightSettings
    sky: OpenUsdDomeLightSettings


class DiagnosticScene:
    def __init__(self, stage: Any) -> None:
        self._stage = stage
        self._governed_sessions = 0
        self._create()

    def enter_governed_session(self) -> None:
        self._governed_sessions += 1
        try:
            self._apply()
        except BaseException:
            self._governed_sessions -= 1
            raise

    def leave_governed_session(self) -> None:
        if self._governed_sessions < 1:
            raise RuntimeError(
                "diagnostic scene governed-session count underflowed"
            )
        self._governed_sessions -= 1
        try:
            self._apply()
        except BaseException:
            self._governed_sessions += 1
            raise

    def isolated(self) -> bool:
        from pxr import UsdGeom, UsdLux

        root = UsdGeom.Imageable(self._stage.GetPrimAtPath(DIAGNOSTIC_ROOT))
        sun = UsdLux.DistantLight.Get(self._stage, DIAGNOSTIC_SUN)
        dome = UsdLux.DomeLight.Get(self._stage, DIAGNOSTIC_DOME)
        if self._governed_sessions == 0:
            return (
                root.GetVisibilityAttr().Get()
                == UsdGeom.Tokens.inherited
                and _close(sun.GetIntensityAttr().Get(), DIAGNOSTIC_SUN_INTENSITY)
                and _close(dome.GetIntensityAttr().Get(), DIAGNOSTIC_DOME_INTENSITY)
            )
        return (
            root.GetVisibilityAttr().Get() == UsdGeom.Tokens.invisible
            and _close(sun.GetIntensityAttr().Get(), 0.0)
            and _close(dome.GetIntensityAttr().Get(), 0.0)
        )

    def _create(self) -> None:
        from pxr import Gf, UsdGeom, UsdLux

        self._stage.DefinePrim("/World/SimulationView", "Xform")
        self._stage.DefinePrim(
            "/World/SimulationView/Sessions", "Scope"
        )
        self._stage.DefinePrim(
            "/World/SimulationView/Cameras", "Scope"
        )
        root = UsdGeom.Xform.Define(self._stage, DIAGNOSTIC_ROOT)
        root.CreateVisibilityAttr(UsdGeom.Tokens.inherited)
        cube = UsdGeom.Cube.Define(
            self._stage, f"{DIAGNOSTIC_ROOT}/Cube"
        )
        cube.CreateSizeAttr(2.0)
        cube.CreateDisplayColorAttr([Gf.Vec3f(0.04, 0.65, 0.85)])
        ground = UsdGeom.Cube.Define(
            self._stage, f"{DIAGNOSTIC_ROOT}/Ground"
        )
        ground.CreateSizeAttr(1.0)
        ground_xform = UsdGeom.Xformable(ground.GetPrim())
        ground_xform.AddScaleOp().Set(Gf.Vec3f(18.0, 18.0, 0.1))
        ground_xform.AddTranslateOp().Set(Gf.Vec3d(0.0, 0.0, -1.05))
        ground.CreateDisplayColorAttr([Gf.Vec3f(0.08, 0.1, 0.14)])
        sun = UsdLux.DistantLight.Define(self._stage, DIAGNOSTIC_SUN)
        sun.CreateIntensityAttr(DIAGNOSTIC_SUN_INTENSITY)
        UsdGeom.Xformable(sun.GetPrim()).AddRotateXYZOp().Set(
            Gf.Vec3f(35.0, -25.0, -30.0)
        )
        dome = UsdLux.DomeLight.Define(self._stage, DIAGNOSTIC_DOME)
        dome.CreateIntensityAttr(DIAGNOSTIC_DOME_INTENSITY)

    def _apply(self) -> None:
        from pxr import UsdGeom, UsdLux

        governed = self._governed_sessions > 0
        root = UsdGeom.Imageable(self._stage.GetPrimAtPath(DIAGNOSTIC_ROOT))
        root.GetVisibilityAttr().Set(
            UsdGeom.Tokens.invisible
            if governed
            else UsdGeom.Tokens.inherited
        )
        UsdLux.DistantLight.Get(
            self._stage, DIAGNOSTIC_SUN
        ).GetIntensityAttr().Set(
            0.0 if governed else DIAGNOSTIC_SUN_INTENSITY
        )
        UsdLux.DomeLight.Get(
            self._stage, DIAGNOSTIC_DOME
        ).GetIntensityAttr().Set(
            0.0 if governed else DIAGNOSTIC_DOME_INTENSITY
        )
        if not self.isolated():
            raise RuntimeError(
                "diagnostic lighting isolation did not take effect"
            )


def author_governed_lighting(
    stage: Any,
    session_root: str,
    lighting: GovernedLighting,
) -> None:
    from pxr import Gf, UsdGeom, UsdLux

    values = lighting.openusd_settings()
    lighting_root = f"{session_root}/Lighting"
    stage.DefinePrim(lighting_root, "Scope")
    sun = UsdLux.DistantLight.Define(stage, f"{lighting_root}/Sun")
    sun.CreateIntensityAttr(values.sun.intensity)
    sun.CreateExposureAttr(values.sun.exposure)
    sun.CreateNormalizeAttr(values.sun.normalize)
    sun.CreateEnableColorTemperatureAttr(
        values.sun.enable_color_temperature
    )
    sun.CreateColorTemperatureAttr(values.sun.color_temperature_kelvin)
    sun.CreateAngleAttr(values.sun.angle_degrees)
    UsdGeom.Xformable(sun.GetPrim()).AddRotateXYZOp().Set(
        Gf.Vec3f(*values.sun.rotation_degrees)
    )
    sky = UsdLux.DomeLight.Define(stage, f"{lighting_root}/Sky")
    sky.CreateIntensityAttr(values.sky.intensity)
    sky.CreateExposureAttr(values.sky.exposure)
    sky.CreateNormalizeAttr(values.sky.normalize)
    sky.CreateEnableColorTemperatureAttr(
        values.sky.enable_color_temperature
    )
    sky.CreateColorTemperatureAttr(values.sky.color_temperature_kelvin)


def _close(actual: object, expected: float) -> bool:
    return isinstance(actual, (int, float)) and math.isclose(
        float(actual), expected, rel_tol=0.0, abs_tol=1.0e-6
    )
