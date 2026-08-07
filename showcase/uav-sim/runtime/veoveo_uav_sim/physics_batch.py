from __future__ import annotations

import math
import time
from collections.abc import Callable, Mapping, Sequence
from dataclasses import dataclass
from typing import Any

import numpy as np


@dataclass(frozen=True, slots=True)
class RigidBodyState:
    position_xyz: np.ndarray
    orientation_xyzw: np.ndarray
    linear_velocity_xyz: np.ndarray
    angular_velocity_xyz: np.ndarray


@dataclass(frozen=True, slots=True)
class FleetPhysicsTiming:
    """Cumulative wall-time attribution for the authoritative physics callback."""

    physics_steps: int
    refresh_states_wall_seconds: float
    vehicle_update_wall_seconds: float
    flush_forces_wall_seconds: float
    after_step_wall_seconds: float
    maximum_physics_step_ms: float


class RigidBodyBatchAccumulator:
    """Allocation-stable host side of the Isaac rigid-body tensor batch."""

    def __init__(
        self,
        body_paths: Sequence[str],
        *,
        forces: np.ndarray | None = None,
        torques: np.ndarray | None = None,
    ) -> None:
        paths = tuple(body_paths)
        if not paths or len(set(paths)) != len(paths):
            raise ValueError("rigid-body batch paths must be nonempty and unique")
        self._paths = paths
        self._indices = {path: index for index, path in enumerate(paths)}
        self.forces = _batch_vector_buffer(forces, len(paths), "force")
        self.torques = _batch_vector_buffer(torques, len(paths), "torque")
        self._transforms = np.zeros((len(paths), 7), dtype=np.float32)
        self._velocities = np.zeros((len(paths), 6), dtype=np.float32)
        self._state_ready = False

    @property
    def paths(self) -> tuple[str, ...]:
        return self._paths

    def update_states(
        self, transforms: np.ndarray, velocities: np.ndarray
    ) -> None:
        expected_transforms = (len(self._paths), 7)
        expected_velocities = (len(self._paths), 6)
        if transforms.shape != expected_transforms:
            raise RuntimeError(
                f"rigid-body transforms have shape {transforms.shape}; "
                f"expected {expected_transforms}"
            )
        if velocities.shape != expected_velocities:
            raise RuntimeError(
                f"rigid-body velocities have shape {velocities.shape}; "
                f"expected {expected_velocities}"
            )
        if not np.isfinite(transforms).all() or not np.isfinite(velocities).all():
            raise RuntimeError("rigid-body state contains a non-finite value")
        np.copyto(self._transforms, transforms, casting="no")
        np.copyto(self._velocities, velocities, casting="no")
        self._state_ready = True

    def state(self, body_path: str) -> RigidBodyState:
        if not self._state_ready:
            raise RuntimeError("rigid-body batch state has not been refreshed")
        index = self._index(body_path)
        transform = self._transforms[index]
        velocity = self._velocities[index]
        return RigidBodyState(
            position_xyz=transform[:3],
            orientation_xyzw=transform[3:7],
            linear_velocity_xyz=velocity[:3],
            angular_velocity_xyz=velocity[3:6],
        )

    def queue_force(
        self,
        body_path: str,
        force_xyz: Sequence[float],
        position_xyz: Sequence[float],
    ) -> None:
        force = _vector3(force_xyz, "force")
        position = _vector3(position_xyz, "force position")
        index = self._index(body_path)
        self.forces[index, 0] += force[0]
        self.forces[index, 1] += force[1]
        self.forces[index, 2] += force[2]
        # A force at p is mechanically equivalent to the same force at the
        # body origin plus p x F. This permits one tensor submission per body.
        self.torques[index, 0] += position[1] * force[2] - position[2] * force[1]
        self.torques[index, 1] += position[2] * force[0] - position[0] * force[2]
        self.torques[index, 2] += position[0] * force[1] - position[1] * force[0]

    def queue_torque(
        self, body_path: str, torque_xyz: Sequence[float]
    ) -> None:
        torque = _vector3(torque_xyz, "torque")
        index = self._index(body_path)
        self.torques[index, 0] += torque[0]
        self.torques[index, 1] += torque[1]
        self.torques[index, 2] += torque[2]

    def clear_forces(self) -> None:
        self.forces.fill(0.0)
        self.torques.fill(0.0)

    def _index(self, path: str) -> int:
        try:
            return self._indices[path]
        except KeyError as error:
            raise RuntimeError(
                f"rigid body {path!r} is outside the admitted fleet batch"
            ) from error


class IsaacFleetPhysicsBatch:
    """One reusable GPU tensor path for all Pegasus vehicle bodies."""

    def __init__(self, body_paths: Sequence[str], physics_view: Any) -> None:
        self._accumulator = RigidBodyBatchAccumulator(body_paths)
        self._physics_view: Any = None
        self._rigid_body_view: Any = None
        self._warp: Any = None
        self._device: Any = None
        self._force_host: Any = None
        self._torque_host: Any = None
        self._force_device: Any = None
        self._torque_device: Any = None
        self._indices_device: Any = None
        self._transforms_host: Any = None
        self._velocities_host: Any = None
        self._transforms_numpy: np.ndarray | None = None
        self._velocities_numpy: np.ndarray | None = None
        self.rebind(physics_view)

    @property
    def device(self) -> str:
        return str(self._device)

    @property
    def body_count(self) -> int:
        return len(self._accumulator.paths)

    def rebind(self, physics_view: Any) -> None:
        import warp as wp

        if physics_view is None:
            raise RuntimeError("Isaac World did not initialize its physics tensor view")
        physics_view.set_subspace_roots("/")
        rigid_body_view = physics_view.create_rigid_body_view(
            list(self._accumulator.paths)
        )
        actual_paths = tuple(rigid_body_view.prim_paths)
        if len(actual_paths) != len(self._accumulator.paths) or set(
            actual_paths
        ) != set(self._accumulator.paths):
            raise RuntimeError(
                "Isaac rigid-body tensor view does not exactly match the fleet; "
                f"expected {self._accumulator.paths}, received {actual_paths}"
            )
        device = wp.get_device(physics_view.device)
        if not device.is_cuda:
            raise RuntimeError(
                "UAV fleet physics requires a CUDA-backed Isaac tensor view"
            )

        # Isaac determines tensor row order. Create Warp-owned host tensors
        # first and write through their NumPy views. ``wp.from_numpy`` copies
        # its input in Warp 1.15; retaining a wrapper created from an initially
        # zero NumPy accumulator would therefore submit frozen zero forces for
        # every later physics step.
        count = len(actual_paths)
        self._physics_view = physics_view
        self._rigid_body_view = rigid_body_view
        self._warp = wp
        self._device = device
        self._force_host = wp.zeros(count * 3, dtype=wp.float32, device="cpu")
        self._torque_host = wp.zeros(count * 3, dtype=wp.float32, device="cpu")
        self._accumulator = RigidBodyBatchAccumulator(
            actual_paths,
            forces=self._force_host.numpy().reshape(count, 3),
            torques=self._torque_host.numpy().reshape(count, 3),
        )
        # PhysX's public tensor frontend accepts the documented contiguous N*3
        # form, so retain flat scalar views on both sides of the reusable copy.
        self._force_device = wp.zeros(count * 3, dtype=wp.float32, device=device)
        self._torque_device = wp.zeros(count * 3, dtype=wp.float32, device=device)
        self._indices_device = wp.array(
            np.arange(count, dtype=np.uint32), dtype=wp.uint32, device=device
        )
        self._transforms_host = wp.zeros(
            (count, 7), dtype=wp.float32, device="cpu"
        )
        self._velocities_host = wp.zeros(
            (count, 6), dtype=wp.float32, device="cpu"
        )
        self._transforms_numpy = self._transforms_host.numpy()
        self._velocities_numpy = self._velocities_host.numpy()

    def refresh_states(self) -> None:
        transforms = self._rigid_body_view.get_transforms()
        velocities = self._rigid_body_view.get_velocities()
        self._warp.copy(self._transforms_host, transforms)
        self._warp.copy(self._velocities_host, velocities)
        self._warp.synchronize_device(self._device)
        assert self._transforms_numpy is not None
        assert self._velocities_numpy is not None
        self._accumulator.update_states(
            self._transforms_numpy, self._velocities_numpy
        )

    def state(self, body_path: str) -> RigidBodyState:
        return self._accumulator.state(body_path)

    def queue_force(
        self,
        body_path: str,
        force_xyz: Sequence[float],
        position_xyz: Sequence[float],
    ) -> None:
        self._accumulator.queue_force(body_path, force_xyz, position_xyz)

    def queue_torque(
        self, body_path: str, torque_xyz: Sequence[float]
    ) -> None:
        self._accumulator.queue_torque(body_path, torque_xyz)

    def flush_forces(self) -> None:
        self._warp.copy(self._force_device, self._force_host)
        self._warp.copy(self._torque_device, self._torque_host)
        # The host buffers are also the writable accumulator. Complete the two
        # small H2D transfers before clearing them for the next physics step.
        self._warp.synchronize_stream(self._device)
        self._rigid_body_view.apply_forces_and_torques_at_position(
            self._force_device,
            self._torque_device,
            None,
            self._indices_device,
            False,
        )
        self._accumulator.clear_forces()


class FleetPhysicsLifecycle:
    """Own the reset-safe transition to one admitted fleet callback."""

    _CALLBACK_NAME = "/World/veoveo_uav_fleet/physics_batch"
    _PEGASUS_CALLBACK_SUFFIXES = ("/state", "/update", "/Sensors", "/mav_state")

    def __init__(
        self,
        world: Any,
        vehicles: Mapping[str, Any],
        callback_prefixes: Mapping[str, str],
        body_paths: Sequence[str],
        batch_factory: Callable[[Sequence[str], Any], Any] = IsaacFleetPhysicsBatch,
        after_step: Callable[[float], None] | None = None,
    ) -> None:
        if set(vehicles) != set(callback_prefixes):
            raise ValueError(
                "fleet vehicles and Pegasus callback prefixes must have identical identities"
            )
        self._world = world
        self._vehicles = vehicles
        self._callback_prefixes = callback_prefixes
        self._body_paths = tuple(body_paths)
        self._batch_factory = batch_factory
        self._after_step = after_step
        self._batch: Any = None
        self._physics_steps = 0
        self._refresh_states_wall_seconds = 0.0
        self._vehicle_update_wall_seconds = 0.0
        self._flush_forces_wall_seconds = 0.0
        self._after_step_wall_seconds = 0.0
        self._maximum_physics_step_ms = 0.0

    @property
    def batch(self) -> Any:
        if self._batch is None:
            raise RuntimeError("fleet physics batch has not been initialized")
        return self._batch

    def timing(self) -> FleetPhysicsTiming:
        return FleetPhysicsTiming(
            physics_steps=self._physics_steps,
            refresh_states_wall_seconds=self._refresh_states_wall_seconds,
            vehicle_update_wall_seconds=self._vehicle_update_wall_seconds,
            flush_forces_wall_seconds=self._flush_forces_wall_seconds,
            after_step_wall_seconds=self._after_step_wall_seconds,
            maximum_physics_step_ms=self._maximum_physics_step_ms,
        )

    def reset(self) -> Any:
        # World.reset advances PhysX. No callback may reference the old tensor
        # stage or an as-yet-unbound batch during that transition.
        self.remove_callbacks()
        self._world.reset()
        physics_view = self._world.physics_sim_view
        if physics_view is None:
            raise RuntimeError("Isaac World reset without a physics tensor view")
        if self._batch is None:
            self._batch = self._batch_factory(self._body_paths, physics_view)
        else:
            self._batch.rebind(physics_view)
        for vehicle in self._vehicles.values():
            vehicle.bind_physics_batch(self._batch)
        self._world.add_physics_callback(self._CALLBACK_NAME, self._update)
        return self._batch

    def remove_callbacks(self) -> None:
        for vehicle_id in self._vehicles:
            prefix = self._callback_prefixes[vehicle_id]
            for suffix in self._PEGASUS_CALLBACK_SUFFIXES:
                callback_name = prefix + suffix
                if self._world.physics_callback_exists(callback_name):
                    self._world.remove_physics_callback(callback_name)
        if self._world.physics_callback_exists(self._CALLBACK_NAME):
            self._world.remove_physics_callback(self._CALLBACK_NAME)

    def _update(self, dt: float) -> None:
        physics_step_started = time.perf_counter()
        batch = self.batch
        phase_started = time.perf_counter()
        batch.refresh_states()
        self._refresh_states_wall_seconds += time.perf_counter() - phase_started

        phase_started = time.perf_counter()
        for vehicle in self._vehicles.values():
            vehicle.update_state(dt)
            vehicle.update(dt)
            vehicle.update_sensors(dt)
            vehicle.update_sim_state(dt)
        self._vehicle_update_wall_seconds += time.perf_counter() - phase_started

        phase_started = time.perf_counter()
        batch.flush_forces()
        self._flush_forces_wall_seconds += time.perf_counter() - phase_started

        phase_started = time.perf_counter()
        if self._after_step is not None:
            self._after_step(dt)
        self._after_step_wall_seconds += time.perf_counter() - phase_started
        self._physics_steps += 1
        self._maximum_physics_step_ms = max(
            self._maximum_physics_step_ms,
            (time.perf_counter() - physics_step_started) * 1_000.0,
        )


def _vector3(value: Sequence[float], label: str) -> tuple[float, float, float]:
    if len(value) != 3:
        raise RuntimeError(f"{label} must contain exactly three components")
    result = (float(value[0]), float(value[1]), float(value[2]))
    if not all(math.isfinite(component) for component in result):
        raise RuntimeError(f"{label} contains a non-finite component")
    return result


def _batch_vector_buffer(
    value: np.ndarray | None, count: int, label: str
) -> np.ndarray:
    if value is None:
        return np.zeros((count, 3), dtype=np.float32)
    if value.shape != (count, 3):
        raise ValueError(
            f"rigid-body {label} buffer has shape {value.shape}; "
            f"expected {(count, 3)}"
        )
    if value.dtype != np.float32 or not value.flags.c_contiguous:
        raise ValueError(
            f"rigid-body {label} buffer must be contiguous float32 storage"
        )
    value.fill(0.0)
    return value
