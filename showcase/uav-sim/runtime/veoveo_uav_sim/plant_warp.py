from __future__ import annotations

import warp as wp

from .vehicle_spec import (
    PX4_IRIS_LINEAR_DRAG_FLU_NS_M,
    PX4_IRIS_MOTOR_CONSTANT,
    PX4_IRIS_ROTOR_DIRECTIONS,
    PX4_IRIS_ROTOR_POSITIONS_FLU_M,
    PX4_IRIS_TIME_CONSTANT_DOWN_S,
    PX4_IRIS_TIME_CONSTANT_UP_S,
    PX4_IRIS_YAW_MOMENT_COEFFICIENT,
)


PACKET_WIDTH = 30
MOTOR_CONSTANT = wp.constant(PX4_IRIS_MOTOR_CONSTANT)
YAW_MOMENT_COEFFICIENT = wp.constant(PX4_IRIS_YAW_MOMENT_COEFFICIENT)
TIME_CONSTANT_UP_S = wp.constant(PX4_IRIS_TIME_CONSTANT_UP_S)
TIME_CONSTANT_DOWN_S = wp.constant(PX4_IRIS_TIME_CONSTANT_DOWN_S)
DRAG_X = wp.constant(PX4_IRIS_LINEAR_DRAG_FLU_NS_M[0])
DRAG_Y = wp.constant(PX4_IRIS_LINEAR_DRAG_FLU_NS_M[1])
ROTOR_0_X = wp.constant(PX4_IRIS_ROTOR_POSITIONS_FLU_M[0][0])
ROTOR_0_Y = wp.constant(PX4_IRIS_ROTOR_POSITIONS_FLU_M[0][1])
ROTOR_1_X = wp.constant(PX4_IRIS_ROTOR_POSITIONS_FLU_M[1][0])
ROTOR_1_Y = wp.constant(PX4_IRIS_ROTOR_POSITIONS_FLU_M[1][1])
ROTOR_2_X = wp.constant(PX4_IRIS_ROTOR_POSITIONS_FLU_M[2][0])
ROTOR_2_Y = wp.constant(PX4_IRIS_ROTOR_POSITIONS_FLU_M[2][1])
ROTOR_3_X = wp.constant(PX4_IRIS_ROTOR_POSITIONS_FLU_M[3][0])
ROTOR_3_Y = wp.constant(PX4_IRIS_ROTOR_POSITIONS_FLU_M[3][1])
ROTOR_0_DIRECTION = wp.constant(PX4_IRIS_ROTOR_DIRECTIONS[0])
ROTOR_1_DIRECTION = wp.constant(PX4_IRIS_ROTOR_DIRECTIONS[1])
ROTOR_2_DIRECTION = wp.constant(PX4_IRIS_ROTOR_DIRECTIONS[2])
ROTOR_3_DIRECTION = wp.constant(PX4_IRIS_ROTOR_DIRECTIONS[3])


@wp.kernel
def update_motor_wrench(
    controls: wp.array2d(dtype=wp.float32),
    motor_velocity: wp.array2d(dtype=wp.float32),
    orientations_wxyz: wp.array2d(dtype=wp.float32),
    linear_velocity_enu: wp.array2d(dtype=wp.float32),
    forces_flu: wp.array2d(dtype=wp.float32),
    torques_flu: wp.array2d(dtype=wp.float32),
    dt: wp.float32,
):
    vehicle = wp.tid()
    force_0 = wp.float32(0.0)
    force_1 = wp.float32(0.0)
    force_2 = wp.float32(0.0)
    force_3 = wp.float32(0.0)

    for rotor in range(4):
        target = wp.clamp(controls[vehicle, rotor], 0.0, 1100.0)
        current = motor_velocity[vehicle, rotor]
        time_constant = TIME_CONSTANT_DOWN_S
        if target > current:
            time_constant = TIME_CONSTANT_UP_S
        blend = 1.0 - wp.exp(-dt / time_constant)
        current = current + blend * (target - current)
        motor_velocity[vehicle, rotor] = current
        thrust = MOTOR_CONSTANT * current * current
        if rotor == 0:
            force_0 = thrust
        elif rotor == 1:
            force_1 = thrust
        elif rotor == 2:
            force_2 = thrust
        else:
            force_3 = thrust

    orientation = wp.quat(
        orientations_wxyz[vehicle, 1],
        orientations_wxyz[vehicle, 2],
        orientations_wxyz[vehicle, 3],
        orientations_wxyz[vehicle, 0],
    )
    velocity_flu = wp.quat_rotate_inv(
        orientation,
        wp.vec3(
            linear_velocity_enu[vehicle, 0],
            linear_velocity_enu[vehicle, 1],
            linear_velocity_enu[vehicle, 2],
        ),
    )
    forces_flu[vehicle, 0] = -DRAG_X * velocity_flu[0]
    forces_flu[vehicle, 1] = -DRAG_Y * velocity_flu[1]
    forces_flu[vehicle, 2] = force_0 + force_1 + force_2 + force_3

    torques_flu[vehicle, 0] = (
        ROTOR_0_Y * force_0
        + ROTOR_1_Y * force_1
        + ROTOR_2_Y * force_2
        + ROTOR_3_Y * force_3
    )
    torques_flu[vehicle, 1] = -(
        ROTOR_0_X * force_0
        + ROTOR_1_X * force_1
        + ROTOR_2_X * force_2
        + ROTOR_3_X * force_3
    )
    torques_flu[vehicle, 2] = YAW_MOMENT_COEFFICIENT * (
        ROTOR_0_DIRECTION * motor_velocity[vehicle, 0] * motor_velocity[vehicle, 0]
        + ROTOR_1_DIRECTION * motor_velocity[vehicle, 1] * motor_velocity[vehicle, 1]
        + ROTOR_2_DIRECTION * motor_velocity[vehicle, 2] * motor_velocity[vehicle, 2]
        + ROTOR_3_DIRECTION * motor_velocity[vehicle, 3] * motor_velocity[vehicle, 3]
    )


@wp.kernel
def sample_hil_sensors(
    positions_enu: wp.array2d(dtype=wp.float32),
    orientations_wxyz: wp.array2d(dtype=wp.float32),
    linear_velocity_enu: wp.array2d(dtype=wp.float32),
    angular_velocity_enu: wp.array2d(dtype=wp.float32),
    previous_linear_velocity_enu: wp.array2d(dtype=wp.float32),
    packet: wp.array2d(dtype=wp.float32),
    dt: wp.float32,
    origin_latitude_degrees: wp.float32,
    origin_longitude_degrees: wp.float32,
    origin_altitude_m: wp.float32,
    meters_per_degree_latitude: wp.float32,
    meters_per_degree_longitude: wp.float32,
):
    vehicle = wp.tid()
    east = positions_enu[vehicle, 0]
    north = positions_enu[vehicle, 1]
    up = positions_enu[vehicle, 2]
    velocity = wp.vec3(
        linear_velocity_enu[vehicle, 0],
        linear_velocity_enu[vehicle, 1],
        linear_velocity_enu[vehicle, 2],
    )
    previous_velocity = wp.vec3(
        previous_linear_velocity_enu[vehicle, 0],
        previous_linear_velocity_enu[vehicle, 1],
        previous_linear_velocity_enu[vehicle, 2],
    )
    orientation = wp.quat(
        orientations_wxyz[vehicle, 1],
        orientations_wxyz[vehicle, 2],
        orientations_wxyz[vehicle, 3],
        orientations_wxyz[vehicle, 0],
    )

    specific_force_enu = (velocity - previous_velocity) / dt - wp.vec3(
        0.0, 0.0, -9.80665
    )
    acceleration_flu = wp.quat_rotate_inv(orientation, specific_force_enu)
    angular_flu = wp.quat_rotate_inv(
        orientation,
        wp.vec3(
            angular_velocity_enu[vehicle, 0],
            angular_velocity_enu[vehicle, 1],
            angular_velocity_enu[vehicle, 2],
        ),
    )
    magnetic_flu = wp.quat_rotate_inv(orientation, wp.vec3(0.0, 0.215, -0.427))

    altitude = origin_altitude_m + up
    temperature_kelvin = wp.max(180.0, 288.15 - 0.0065 * altitude)
    pressure_hpa = 1013.25 / wp.pow(288.15 / temperature_kelvin, 5.2561)
    latitude = origin_latitude_degrees + north / meters_per_degree_latitude
    longitude = origin_longitude_degrees + east / meters_per_degree_longitude
    ground_speed = wp.sqrt(velocity[0] * velocity[0] + velocity[1] * velocity[1])
    course = wp.atan2(velocity[0], velocity[1]) * 57.29577951308232
    if course < 0.0:
        course = course + 360.0

    packet[vehicle, 0] = east
    packet[vehicle, 1] = north
    packet[vehicle, 2] = up
    packet[vehicle, 3] = orientations_wxyz[vehicle, 1]
    packet[vehicle, 4] = orientations_wxyz[vehicle, 2]
    packet[vehicle, 5] = orientations_wxyz[vehicle, 3]
    packet[vehicle, 6] = orientations_wxyz[vehicle, 0]
    packet[vehicle, 7] = velocity[0]
    packet[vehicle, 8] = velocity[1]
    packet[vehicle, 9] = velocity[2]
    packet[vehicle, 10] = angular_flu[0]
    packet[vehicle, 11] = -angular_flu[1]
    packet[vehicle, 12] = -angular_flu[2]
    packet[vehicle, 13] = acceleration_flu[0]
    packet[vehicle, 14] = -acceleration_flu[1]
    packet[vehicle, 15] = -acceleration_flu[2]
    packet[vehicle, 16] = magnetic_flu[0]
    packet[vehicle, 17] = -magnetic_flu[1]
    packet[vehicle, 18] = -magnetic_flu[2]
    packet[vehicle, 19] = pressure_hpa
    packet[vehicle, 20] = altitude
    packet[vehicle, 21] = temperature_kelvin - 273.15
    packet[vehicle, 22] = latitude
    packet[vehicle, 23] = longitude
    packet[vehicle, 24] = altitude
    packet[vehicle, 25] = velocity[1]
    packet[vehicle, 26] = velocity[0]
    packet[vehicle, 27] = -velocity[2]
    packet[vehicle, 28] = ground_speed
    packet[vehicle, 29] = course

    previous_linear_velocity_enu[vehicle, 0] = velocity[0]
    previous_linear_velocity_enu[vehicle, 1] = velocity[1]
    previous_linear_velocity_enu[vehicle, 2] = velocity[2]
