"""Bind completed external GPU dynamics to Isaac's render-reference clock."""

from __future__ import annotations

import math
import logging
from collections.abc import Callable
from dataclasses import dataclass
from typing import Protocol


class FabricTimeAttribute(Protocol):
    def Set(self, value: float) -> object: ...
    def Get(self) -> float: ...


class ClockSettings(Protocol):
    def set_bool(self, path: str, value: bool) -> None: ...
    def get_as_bool(self, path: str) -> bool: ...


class SimulationClockInterface(Protocol):
    def get_sample_count(self) -> int: ...
    def get_num_physics_steps(self) -> int: ...
    def get_simulation_time_at_time(self, time: tuple[int, int]) -> float: ...


@dataclass(frozen=True, slots=True)
class RenderClockObservation:
    generation: int
    physics_step: int
    simulation_time_s: float
    native_physics_steps: int
    sample_count: int


class ExternalSimulationClock:
    def __init__(
        self,
        attribute: FabricTimeAttribute,
        settings: ClockSettings,
        manager: SimulationClockInterface,
        physics_hz: int,
    ) -> None:
        if physics_hz <= 0:
            raise ValueError("external clock requires positive physics frequency")
        self._attribute = attribute
        self._settings = settings
        self._manager = manager
        self._physics_hz = physics_hz
        self._last_step = 0
        self._generation = 1
        self._admitted = False
        self.last_observation: RenderClockObservation | None = None
        settings.set_bool("/physics/resetOnStop", False)
        settings.set_bool("/rtx/hydra/supportMultiTickRate", True)
        attribute.Set(0.0)

    @classmethod
    def create(cls, physics_hz: int) -> ExternalSimulationClock:
        import carb
        import isaacsim.core.experimental.utils.prim as prim_utils
        import isaacsim.core.experimental.utils.stage as stage_utils
        from isaacsim.core.simulation_manager import _simulation_manager
        from pxr import Sdf

        stage = stage_utils.get_current_stage(backend="fabric")
        prim = stage.GetPrimAtPath("/ExternalSimulationTime")
        if not prim:
            prim = stage.DefinePrim("/ExternalSimulationTime", "")
        attribute = prim.GetAttribute("omni:time")
        if not attribute:
            attribute = prim_utils.create_prim_attribute(
                prim, name="omni:time", type_name=Sdf.ValueTypeNames.Double
            )
        return cls(attribute, carb.settings.get_settings(),
                   _simulation_manager.acquire_simulation_manager_interface(), physics_hz)

    def admit_initialized(self) -> None:
        if self._manager.get_sample_count() <= 0:
            raise RuntimeError("external render clock requires initialized simulation-time samples")
        if not self._settings.get_as_bool("/rtx/hydra/supportMultiTickRate"):
            raise RuntimeError("external render clock requires multi-tick rendering")
        self._admitted = True

    def render(self, physics_step: int, update: Callable[[], None]) -> RenderClockObservation:
        try:
            return self._render(physics_step, update)
        except BaseException:
            self.retire_generation()
            raise

    def _render(self, physics_step: int, update: Callable[[], None]) -> RenderClockObservation:
        if not self._admitted:
            raise RuntimeError("external render clock is not initialized")
        if physics_step < self._last_step:
            raise RuntimeError("external render clock requires an explicit generation reset")
        self.admit_initialized()
        simulation_time_s = physics_step / self._physics_hz
        self._attribute.Set(simulation_time_s)
        actual_time = self._attribute.Get()
        if not math.isclose(actual_time, simulation_time_s, rel_tol=0.0, abs_tol=1e-9):
            raise RuntimeError("external Fabric clock did not retain completed dynamics time")
        reference = self._manager.get_simulation_time_at_time((physics_step, self._physics_hz))
        if not math.isclose(reference, simulation_time_s, rel_tol=0.0, abs_tol=1e-9):
            raise RuntimeError("external render reference does not match completed dynamics")
        native_steps = self._manager.get_num_physics_steps()
        previous = self._settings.get_as_bool("/app/player/playSimulations")
        self._settings.set_bool("/app/player/playSimulations", False)
        try:
            update()
        finally:
            self._settings.set_bool("/app/player/playSimulations", previous)
        if self._manager.get_num_physics_steps() != native_steps:
            raise RuntimeError("render advanced native physics alongside external dynamics")
        self.admit_initialized()
        if not math.isclose(self._attribute.Get(), simulation_time_s, rel_tol=0.0, abs_tol=1e-9):
            raise RuntimeError("render changed the completed external Fabric clock")
        first_observation = self.last_observation is None
        self._last_step = physics_step
        self.last_observation = RenderClockObservation(
            self._generation, physics_step, simulation_time_s, native_steps,
            self._manager.get_sample_count(),
        )
        if first_observation:
            logging.getLogger("veoveo.uav_sim.external_clock").info(
                "external render clock admitted generation=%d completed_step=%d "
                "physics_hz=%d fabric_time_s=%.9f sample_count=%d native_steps=%d",
                self._generation, physics_step, self._physics_hz, actual_time,
                self.last_observation.sample_count, native_steps,
            )
        return self.last_observation

    def retire_generation(self) -> None:
        # Invalidate publication before any fallible stream teardown.
        self._admitted = False
        self.last_observation = None

    def reset_generation(
        self,
        stop: Callable[[], None],
        play: Callable[[], None],
        update: Callable[[], None],
        reset_dynamics: Callable[[], None],
        tensor_identity: Callable[[], tuple[object, ...]],
    ) -> None:
        """Reset normal timeline samples while preserving external tensor owners.

        Writers must already be detached and streams closed by the caller.
        Newton's public resetOnStop=False skips native model initialization.
        """
        if self._settings.get_as_bool("/physics/resetOnStop"):
            raise RuntimeError("external dynamics require resetOnStop disabled")
        identity = tensor_identity()
        self._admitted = False
        previous = self._settings.get_as_bool("/app/player/playSimulations")
        self._settings.set_bool("/app/player/playSimulations", False)
        try:
            stop()
            update()
            if any(a is not b for a, b in zip(identity, tensor_identity(), strict=True)):
                raise RuntimeError("timeline stop replaced external dynamics tensors")
            reset_dynamics()
            self._attribute.Set(0.0)
            play()
            update()
            if any(a is not b for a, b in zip(identity, tensor_identity(), strict=True)):
                raise RuntimeError("timeline play replaced external dynamics tensors")
            # The supported stop lifecycle resets the native step counter too;
            # resume seeds time samples without executing dynamics.
            if self._manager.get_num_physics_steps() != 0:
                raise RuntimeError("timeline reset advanced native dynamics")
            self.admit_initialized()
        finally:
            self._settings.set_bool("/app/player/playSimulations", previous)
        self._generation += 1
        self._last_step = 0
        self.last_observation = None
