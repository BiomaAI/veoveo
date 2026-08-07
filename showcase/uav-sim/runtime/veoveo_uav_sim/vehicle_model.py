from __future__ import annotations

import math
from collections.abc import Sequence
from dataclasses import dataclass

import numpy as np


PX4_IRIS_MOTOR_CONSTANT = 5.84e-6
PX4_IRIS_MOMENT_CONSTANT = 0.06
PX4_IRIS_YAW_MOMENT_COEFFICIENT = (
    PX4_IRIS_MOTOR_CONSTANT * PX4_IRIS_MOMENT_CONSTANT
)
PX4_IRIS_ROTOR_DIRECTIONS = (-1, -1, 1, 1)
PX4_IRIS_MIN_ROTOR_VELOCITY_RPS = 0.0
PX4_IRIS_MAX_ROTOR_VELOCITY_RPS = 1100.0
PX4_IRIS_TIME_CONSTANT_UP_S = 0.0125
PX4_IRIS_TIME_CONSTANT_DOWN_S = 0.025
_SQRT_HALF = math.sqrt(0.5)
_ENU_TO_NED_QUATERNION_XYZW = np.array(
    [_SQRT_HALF, _SQRT_HALF, 0.0, 0.0], dtype=np.float64
)
_FLU_TO_FRD_QUATERNION_XYZW = np.array(
    [1.0, 0.0, 0.0, 0.0], dtype=np.float64
)


def quaternion_multiply_xyzw(
    left: Sequence[float], right: Sequence[float]
) -> np.ndarray:
    """Multiply two XYZW quaternions without constructing SciPy rotations."""
    lx, ly, lz, lw = left
    rx, ry, rz, rw = right
    return np.array(
        [
            lw * rx + lx * rw + ly * rz - lz * ry,
            lw * ry - lx * rz + ly * rw + lz * rx,
            lw * rz + lx * ry - ly * rx + lz * rw,
            lw * rw - lx * rx - ly * ry - lz * rz,
        ],
        dtype=np.float64,
    )


def inverse_rotate_vector_xyzw(
    quaternion: Sequence[float], vector: Sequence[float]
) -> np.ndarray:
    """Rotate a vector by an XYZW quaternion inverse with scalar arithmetic."""
    qx, qy, qz, qw = quaternion
    vx, vy, vz = vector
    # q^-1 * v * q, reduced to two cross products. Isaac supplies normalized
    # rigid-body quaternions, so no normalization or matrix allocation belongs
    # in this physics-step path.
    tx = 2.0 * (-qy * vz + qz * vy)
    ty = 2.0 * (-qz * vx + qx * vz)
    tz = 2.0 * (-qx * vy + qy * vx)
    return np.array(
        [
            vx + qw * tx + (-qy * tz + qz * ty),
            vy + qw * ty + (-qz * tx + qx * tz),
            vz + qw * tz + (-qx * ty + qy * tx),
        ],
        dtype=np.float64,
    )


def enu_to_ned_vector(vector: Sequence[float]) -> np.ndarray:
    east, north, up = vector
    return np.array([north, east, -up], dtype=np.float64)


def flu_to_frd_vector(vector: Sequence[float]) -> np.ndarray:
    forward, left, up = vector
    return np.array([forward, -left, -up], dtype=np.float64)


def attitude_enu_flu_to_ned_frd(attitude_xyzw: Sequence[float]) -> np.ndarray:
    """Convert an authoritative ENU/FLU attitude to the PX4 NED/FRD frame."""
    return quaternion_multiply_xyzw(
        quaternion_multiply_xyzw(
            _ENU_TO_NED_QUATERNION_XYZW, attitude_xyzw
        ),
        _FLU_TO_FRD_QUATERNION_XYZW,
    )


@dataclass(frozen=True)
class Px4IrisSensorCadence:
    """Bounded sensor rates for the authoritative PX4 Iris simulation."""

    imu_hz: int = 60
    barometer_hz: int = 30
    magnetometer_hz: int = 30
    gps_hz: int = 10

    def validate_for_physics(self, physics_hz: int) -> None:
        if physics_hz <= 0:
            raise ValueError("physics cadence must be positive")
        for name, rate_hz in (
            ("IMU", self.imu_hz),
            ("barometer", self.barometer_hz),
            ("magnetometer", self.magnetometer_hz),
            ("GPS", self.gps_hz),
        ):
            if rate_hz <= 0:
                raise ValueError(f"{name} cadence must be positive")
            if rate_hz > physics_hz:
                raise ValueError(
                    f"{name} cadence {rate_hz} Hz exceeds physics cadence "
                    f"{physics_hz} Hz"
                )
            if physics_hz % rate_hz != 0:
                raise ValueError(
                    f"{name} cadence {rate_hz} Hz must divide physics cadence "
                    f"{physics_hz} Hz exactly"
                )


PX4_IRIS_SENSOR_CADENCE = Px4IrisSensorCadence()


class Px4IrisThrustCurve:
    """Pegasus thrust interface with the exact pinned PX4 Iris motor model."""

    def __init__(self) -> None:
        self._num_rotors = 4
        self._rotor_constant = [PX4_IRIS_MOTOR_CONSTANT] * self._num_rotors
        self._rolling_moment_coefficient = [
            PX4_IRIS_YAW_MOMENT_COEFFICIENT
        ] * self._num_rotors
        self._rot_dir = list(PX4_IRIS_ROTOR_DIRECTIONS)
        self.min_rotor_velocity = [
            PX4_IRIS_MIN_ROTOR_VELOCITY_RPS
        ] * self._num_rotors
        self.max_rotor_velocity = [
            PX4_IRIS_MAX_ROTOR_VELOCITY_RPS
        ] * self._num_rotors
        self._input_reference = np.zeros(self._num_rotors, dtype=np.float64)
        self._velocity = np.zeros(self._num_rotors, dtype=np.float64)
        self._force = np.zeros(self._num_rotors, dtype=np.float64)
        self._rolling_moment = 0.0

    def set_input_reference(self, input_reference: Sequence[float]) -> None:
        if len(input_reference) != self._num_rotors:
            raise ValueError("PX4 Iris requires exactly four rotor references")
        references = np.asarray(input_reference, dtype=np.float64)
        if not np.isfinite(references).all():
            raise ValueError("PX4 Iris rotor references must be finite")
        self._input_reference = references

    def update(
        self, _state: object, dt: float
    ) -> tuple[list[float], list[float], float]:
        if not math.isfinite(dt) or dt <= 0.0:
            raise ValueError("PX4 Iris motor timestep must be positive and finite")
        target = np.clip(
            self._input_reference,
            PX4_IRIS_MIN_ROTOR_VELOCITY_RPS,
            PX4_IRIS_MAX_ROTOR_VELOCITY_RPS,
        )
        accelerating = target > self._velocity
        time_constants = np.where(
            accelerating,
            PX4_IRIS_TIME_CONSTANT_UP_S,
            PX4_IRIS_TIME_CONSTANT_DOWN_S,
        )
        blend = 1.0 - np.exp(-dt / time_constants)
        self._velocity += blend * (target - self._velocity)
        self._force = PX4_IRIS_MOTOR_CONSTANT * np.square(self._velocity)
        self._rolling_moment = float(
            np.dot(
                np.asarray(PX4_IRIS_ROTOR_DIRECTIONS, dtype=np.float64),
                PX4_IRIS_YAW_MOMENT_COEFFICIENT * np.square(self._velocity),
            )
        )
        return self.force, self.velocity, self.rolling_moment

    @property
    def force(self) -> list[float]:
        return self._force.tolist()

    @property
    def velocity(self) -> list[float]:
        return self._velocity.tolist()

    @property
    def rolling_moment(self) -> float:
        return self._rolling_moment

    @property
    def rot_dir(self) -> list[int]:
        return list(self._rot_dir)
