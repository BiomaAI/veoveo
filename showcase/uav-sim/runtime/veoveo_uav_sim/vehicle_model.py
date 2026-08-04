from __future__ import annotations

import math
from collections.abc import Sequence

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
