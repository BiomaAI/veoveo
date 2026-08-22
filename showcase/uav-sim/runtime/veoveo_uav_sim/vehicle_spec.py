from __future__ import annotations

import math
from collections.abc import Sequence
from dataclasses import dataclass


PX4_IRIS_MASS_KG = 1.5
PX4_IRIS_DIAGONAL_INERTIA_KG_M2 = (0.029125, 0.029125, 0.055225)
PX4_IRIS_MOTOR_CONSTANT = 5.84e-6
PX4_IRIS_MOMENT_CONSTANT = 0.06
PX4_IRIS_YAW_MOMENT_COEFFICIENT = PX4_IRIS_MOTOR_CONSTANT * PX4_IRIS_MOMENT_CONSTANT
PX4_IRIS_ROTOR_DIRECTIONS = (-1.0, -1.0, 1.0, 1.0)
PX4_IRIS_ROTOR_POSITIONS_FLU_M = (
    (0.13798545, -0.20671639, 0.023),
    (-0.12511168, 0.21875414, 0.023),
    (0.138, 0.20257577, 0.023),
    (-0.1241536, -0.2223458, 0.023),
)
PX4_IRIS_MIN_ROTOR_VELOCITY_RPS = 0.0
PX4_IRIS_MAX_ROTOR_VELOCITY_RPS = 1100.0
PX4_IRIS_TIME_CONSTANT_UP_S = 0.0125
PX4_IRIS_TIME_CONSTANT_DOWN_S = 0.025
PX4_IRIS_LINEAR_DRAG_FLU_NS_M = (0.50, 0.30, 0.0)


@dataclass(frozen=True, slots=True)
class SensorCadence:
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

    def fields_updated(self, physics_hz: int, physics_step: int) -> int:
        fields = 0
        if physics_step % (physics_hz // self.imu_hz) == 0:
            fields |= 7 | 56
        if physics_step % (physics_hz // self.magnetometer_hz) == 0:
            fields |= 448
        if physics_step % (physics_hz // self.barometer_hz) == 0:
            fields |= 6656
        return fields

    def gps_due(self, physics_hz: int, physics_step: int) -> bool:
        return physics_step % (physics_hz // self.gps_hz) == 0


PX4_IRIS_SENSOR_CADENCE = SensorCadence()


@dataclass(frozen=True, slots=True)
class VehicleSnapshot:
    position_enu_m: tuple[float, float, float]
    attitude_xyzw: tuple[float, float, float, float]
    linear_velocity_enu_mps: tuple[float, float, float]
    angular_velocity_frd_rps: tuple[float, float, float]
    linear_acceleration_frd_mps2: tuple[float, float, float]


@dataclass(frozen=True, slots=True)
class HilSensorFrame:
    time_usec: int
    fields_updated: int
    gps_updated: bool
    acceleration_frd_mps2: tuple[float, float, float]
    angular_velocity_frd_rps: tuple[float, float, float]
    magnetic_field_frd_gauss: tuple[float, float, float]
    absolute_pressure_hpa: float
    differential_pressure_hpa: float
    pressure_altitude_m: float
    temperature_celsius: float
    latitude_degrees: float
    longitude_degrees: float
    altitude_m: float
    velocity_ned_mps: tuple[float, float, float]
    ground_speed_mps: float
    course_over_ground_degrees: float
    fix_type: int = 3
    eph_m: float = 1.0
    epv_m: float = 1.0
    satellites_visible: int = 10


def decode_actuator_controls(
    controls: Sequence[float], mode: int, armed_flag: int
) -> tuple[float, float, float, float]:
    if len(controls) < 4:
        raise ValueError("PX4 HIL actuator frame must contain four rotor controls")
    if mode & armed_flag == 0:
        return (0.0, 0.0, 0.0, 0.0)
    decoded = tuple(
        min(
            PX4_IRIS_MAX_ROTOR_VELOCITY_RPS,
            max(PX4_IRIS_MIN_ROTOR_VELOCITY_RPS, float(value) * 1000.0 + 100.0),
        )
        for value in controls[:4]
    )
    if not all(math.isfinite(value) for value in decoded):
        raise ValueError("PX4 HIL actuator frame contains a non-finite value")
    return decoded  # type: ignore[return-value]
