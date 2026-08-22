from __future__ import annotations

import math
import time
from collections.abc import Callable, Sequence
from dataclasses import dataclass

from .px4_hil import Px4HilFleet
from .vehicle_spec import (
    HilSensorFrame,
    PX4_IRIS_SENSOR_CADENCE,
    VehicleSnapshot,
)


@dataclass(frozen=True, slots=True)
class FleetPhysicsTiming:
    physics_steps: int
    refresh_states_wall_seconds: float
    vehicle_update_wall_seconds: float
    state_update_wall_seconds: float
    dynamics_update_wall_seconds: float
    sensor_update_wall_seconds: float
    backend_state_wall_seconds: float
    flush_forces_wall_seconds: float
    after_step_wall_seconds: float
    maximum_physics_step_ms: float


class NewtonFleetRuntime:
    """One Newton rigid-body view and one CUDA Warp plant for the fleet."""

    def __init__(
        self,
        body_paths: Sequence[str],
        initial_positions_enu_m: Sequence[tuple[float, float, float]],
        origin_latitude_degrees: float,
        origin_longitude_degrees: float,
        origin_altitude_m: float,
        physics_hz: int,
        hil: Px4HilFleet,
        after_step: Callable[[float], None] | None = None,
    ) -> None:
        import warp as wp
        from isaacsim.core.experimental.prims import RigidPrim
        from isaacsim.core.simulation_manager import SimulationManager

        from . import plant_warp

        paths = tuple(body_paths)
        initial_positions = tuple(initial_positions_enu_m)
        if not paths or len(paths) != len(initial_positions):
            raise ValueError("Newton fleet paths and initial positions must align")
        if len(paths) != hil.vehicle_count:
            raise ValueError("Newton fleet and PX4 HIL fleet sizes must align")
        PX4_IRIS_SENSOR_CADENCE.validate_for_physics(physics_hz)

        self._wp = wp
        self._kernels = plant_warp
        self._simulation_manager = SimulationManager
        self._paths = paths
        self._initial_positions = initial_positions
        self._physics_hz = physics_hz
        self._dt = 1.0 / physics_hz
        self._hil = hil
        self._after_step = after_step
        self._rigid = RigidPrim(list(paths), resolve_paths=True)
        if len(self._rigid) != len(paths):
            raise RuntimeError(
                f"experimental rigid view resolved {len(self._rigid)} bodies, "
                f"expected {len(paths)}"
            )
        if not self._rigid.is_physics_tensor_entity_valid():
            raise RuntimeError("Newton experimental rigid tensor entity is invalid")
        positions, _ = self._rigid.get_world_poses()
        self._device = wp.get_device(positions.device)
        if not self._device.is_cuda:
            raise RuntimeError("Newton UAV fleet requires a CUDA tensor device")

        count = len(paths)
        self._control_host = wp.zeros((count, 4), dtype=wp.float32, device="cpu")
        self._control_host_values = self._control_host.numpy()
        self._controls = wp.zeros((count, 4), dtype=wp.float32, device=self._device)
        self._motor_velocity = wp.zeros(
            (count, 4), dtype=wp.float32, device=self._device
        )
        self._forces = wp.zeros((count, 3), dtype=wp.float32, device=self._device)
        self._torques = wp.zeros((count, 3), dtype=wp.float32, device=self._device)
        self._previous_linear_velocity = wp.zeros(
            (count, 3), dtype=wp.float32, device=self._device
        )
        self._packet_device = wp.zeros(
            (count, plant_warp.PACKET_WIDTH),
            dtype=wp.float32,
            device=self._device,
        )
        self._packet_host = wp.zeros(
            (count, plant_warp.PACKET_WIDTH), dtype=wp.float32, device="cpu"
        )
        self._packet_host_values = self._packet_host.numpy()

        latitude_radians = math.radians(origin_latitude_degrees)
        self._origin_latitude_degrees = origin_latitude_degrees
        self._origin_longitude_degrees = origin_longitude_degrees
        self._origin_altitude_m = origin_altitude_m
        self._meters_per_degree_latitude = (
            111_132.92
            - 559.82 * math.cos(2.0 * latitude_radians)
            + 1.175 * math.cos(4.0 * latitude_radians)
        )
        self._meters_per_degree_longitude = 111_412.84 * math.cos(
            latitude_radians
        ) - 93.5 * math.cos(3.0 * latitude_radians)
        if self._meters_per_degree_longitude <= 1.0:
            raise ValueError("georeference is too close to a pole for ENU GPS")

        self._snapshots = tuple(
            VehicleSnapshot(
                position_enu_m=position,
                attitude_xyzw=(0.0, 0.0, 0.0, 1.0),
                linear_velocity_enu_mps=(0.0, 0.0, 0.0),
                angular_velocity_frd_rps=(0.0, 0.0, 0.0),
                linear_acceleration_frd_mps2=(0.0, 0.0, -9.80665),
            )
            for position in initial_positions
        )
        self._physics_steps = 0
        self._refresh_states_wall_seconds = 0.0
        self._vehicle_update_wall_seconds = 0.0
        self._state_update_wall_seconds = 0.0
        self._dynamics_update_wall_seconds = 0.0
        self._sensor_update_wall_seconds = 0.0
        self._backend_state_wall_seconds = 0.0
        self._flush_forces_wall_seconds = 0.0
        self._after_step_wall_seconds = 0.0
        self._maximum_physics_step_ms = 0.0
        self.reset()

    @property
    def device(self) -> str:
        return str(self._device)

    @property
    def body_count(self) -> int:
        return len(self._paths)

    def snapshots(self) -> tuple[VehicleSnapshot, ...]:
        return self._snapshots

    def timing(self) -> FleetPhysicsTiming:
        return FleetPhysicsTiming(
            physics_steps=self._physics_steps,
            refresh_states_wall_seconds=self._refresh_states_wall_seconds,
            vehicle_update_wall_seconds=self._vehicle_update_wall_seconds,
            state_update_wall_seconds=self._state_update_wall_seconds,
            dynamics_update_wall_seconds=self._dynamics_update_wall_seconds,
            sensor_update_wall_seconds=self._sensor_update_wall_seconds,
            backend_state_wall_seconds=self._backend_state_wall_seconds,
            flush_forces_wall_seconds=self._flush_forces_wall_seconds,
            after_step_wall_seconds=self._after_step_wall_seconds,
            maximum_physics_step_ms=self._maximum_physics_step_ms,
        )

    def reset(self) -> None:
        count = len(self._paths)
        orientations = [(1.0, 0.0, 0.0, 0.0)] * count
        positions = self._wp.array(
            self._initial_positions, dtype=self._wp.float32, device=self._device
        )
        rotations = self._wp.array(
            orientations, dtype=self._wp.float32, device=self._device
        )
        zeros = self._wp.zeros((count, 3), dtype=self._wp.float32, device=self._device)
        self._rigid.set_world_poses(positions=positions, orientations=rotations)
        self._rigid.set_velocities(linear_velocities=zeros, angular_velocities=zeros)
        self._controls.zero_()
        self._motor_velocity.zero_()
        self._forces.zero_()
        self._torques.zero_()
        self._previous_linear_velocity.zero_()

    def step(self, physics_step: int) -> None:
        started = time.perf_counter()
        self._hil.raise_if_failed()

        phase = time.perf_counter()
        controls = self._hil.controls()
        for vehicle, values in enumerate(controls):
            for rotor, value in enumerate(values):
                self._control_host_values[vehicle, rotor] = value
        self._wp.copy(self._controls, self._control_host)
        positions, orientations = self._rigid.get_world_poses()
        linear_velocity, angular_velocity = self._rigid.get_velocities()
        self._state_update_wall_seconds += time.perf_counter() - phase

        phase = time.perf_counter()
        self._wp.launch(
            self._kernels.update_motor_wrench,
            dim=len(self._paths),
            inputs=[
                self._controls,
                self._motor_velocity,
                orientations,
                linear_velocity,
                self._forces,
                self._torques,
                self._dt,
            ],
            device=self._device,
        )
        self._dynamics_update_wall_seconds += time.perf_counter() - phase

        phase = time.perf_counter()
        self._rigid.apply_forces_and_torques_at_pos(
            self._forces,
            self._torques,
            local_frame=True,
        )
        self._simulation_manager.step(steps=1, update_fabric=False)
        self._flush_forces_wall_seconds += time.perf_counter() - phase

        phase = time.perf_counter()
        positions, orientations = self._rigid.get_world_poses()
        linear_velocity, angular_velocity = self._rigid.get_velocities()
        self._refresh_states_wall_seconds += time.perf_counter() - phase

        phase = time.perf_counter()
        self._wp.launch(
            self._kernels.sample_hil_sensors,
            dim=len(self._paths),
            inputs=[
                positions,
                orientations,
                linear_velocity,
                angular_velocity,
                self._previous_linear_velocity,
                self._packet_device,
                self._dt,
                self._origin_latitude_degrees,
                self._origin_longitude_degrees,
                self._origin_altitude_m,
                self._meters_per_degree_latitude,
                self._meters_per_degree_longitude,
            ],
            device=self._device,
        )
        self._wp.copy(self._packet_host, self._packet_device)
        self._wp.synchronize_stream(self._device)
        frames, snapshots = self._decode_packets(physics_step)
        self._snapshots = snapshots
        self._sensor_update_wall_seconds += time.perf_counter() - phase

        phase = time.perf_counter()
        self._hil.exchange(frames, timeout=max(0.25, 8.0 * self._dt))
        self._backend_state_wall_seconds += time.perf_counter() - phase
        self._vehicle_update_wall_seconds += time.perf_counter() - started

        phase = time.perf_counter()
        if self._after_step is not None:
            self._after_step(self._dt)
        self._after_step_wall_seconds += time.perf_counter() - phase
        self._physics_steps += 1
        self._maximum_physics_step_ms = max(
            self._maximum_physics_step_ms,
            (time.perf_counter() - started) * 1000.0,
        )

    def _decode_packets(
        self, physics_step: int
    ) -> tuple[tuple[HilSensorFrame, ...], tuple[VehicleSnapshot, ...]]:
        time_usec = int(round(physics_step * 1_000_000.0 / self._physics_hz))
        fields_updated = PX4_IRIS_SENSOR_CADENCE.fields_updated(
            self._physics_hz, physics_step
        )
        gps_updated = PX4_IRIS_SENSOR_CADENCE.gps_due(self._physics_hz, physics_step)
        frames: list[HilSensorFrame] = []
        snapshots: list[VehicleSnapshot] = []
        for row in self._packet_host_values:
            values = tuple(float(value) for value in row)
            if not all(math.isfinite(value) for value in values):
                raise RuntimeError("CUDA UAV sensor packet contains a non-finite value")
            acceleration = values[13:16]
            angular_velocity = values[10:13]
            snapshots.append(
                VehicleSnapshot(
                    position_enu_m=values[0:3],
                    attitude_xyzw=values[3:7],
                    linear_velocity_enu_mps=values[7:10],
                    angular_velocity_frd_rps=angular_velocity,
                    linear_acceleration_frd_mps2=acceleration,
                )
            )
            frames.append(
                HilSensorFrame(
                    time_usec=time_usec,
                    fields_updated=fields_updated,
                    gps_updated=gps_updated,
                    acceleration_frd_mps2=acceleration,
                    angular_velocity_frd_rps=angular_velocity,
                    magnetic_field_frd_gauss=values[16:19],
                    absolute_pressure_hpa=values[19],
                    differential_pressure_hpa=0.0,
                    pressure_altitude_m=values[20],
                    temperature_celsius=values[21],
                    latitude_degrees=values[22],
                    longitude_degrees=values[23],
                    altitude_m=values[24],
                    velocity_ned_mps=values[25:28],
                    ground_speed_mps=values[28],
                    course_over_ground_degrees=values[29],
                )
            )
        return tuple(frames), tuple(snapshots)
