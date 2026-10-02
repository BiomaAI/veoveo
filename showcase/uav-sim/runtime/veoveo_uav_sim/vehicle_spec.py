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
PX4_HIL_HZ = 60
# SIH uses min(IMU_INTEG_RATE, IMU_GYRO_RATEMAX); the pinned Iris profile is 250 Hz.
PX4_IRIS_IMU_NOISE_REFERENCE_HZ = 250
HIL_PACKET_WIDTH = 36


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


def decode_hil_packet(
    packet: Sequence[float], *, time_usec: int, fields_updated: int, gps_updated: bool
) -> tuple[HilSensorFrame, VehicleSnapshot]:
    """Decode CUDA truth and measured IMU fields into their separate contracts."""
    values = tuple(float(value) for value in packet)
    if len(values) != HIL_PACKET_WIDTH or not all(math.isfinite(value) for value in values):
        raise ValueError("CUDA UAV sensor packet must contain 36 finite values")
    snapshot = VehicleSnapshot(
        position_enu_m=values[0:3], attitude_xyzw=values[3:7],
        linear_velocity_enu_mps=values[7:10], angular_velocity_frd_rps=values[10:13],
        linear_acceleration_frd_mps2=values[13:16],
    )
    frame = HilSensorFrame(
        time_usec=time_usec, fields_updated=fields_updated, gps_updated=gps_updated,
        angular_velocity_frd_rps=values[30:33], acceleration_frd_mps2=values[33:36],
        magnetic_field_frd_gauss=values[16:19], absolute_pressure_hpa=values[19],
        differential_pressure_hpa=0.0, pressure_altitude_m=values[20],
        temperature_celsius=values[21], latitude_degrees=values[22],
        longitude_degrees=values[23], altitude_m=values[24],
        velocity_ned_mps=values[25:28], ground_speed_mps=values[28],
        course_over_ground_degrees=values[29],
    )
    return frame, snapshot


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
