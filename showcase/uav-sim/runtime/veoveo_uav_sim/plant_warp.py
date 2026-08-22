from __future__ import annotations

import warp as wp

from .vehicle_spec import (
    PX4_IRIS_DIAGONAL_INERTIA_KG_M2,
    PX4_IRIS_LINEAR_DRAG_FLU_NS_M,
    PX4_IRIS_MASS_KG,
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
MASS_KG = wp.constant(PX4_IRIS_MASS_KG)
INERTIA_X = wp.constant(PX4_IRIS_DIAGONAL_INERTIA_KG_M2[0])
INERTIA_Y = wp.constant(PX4_IRIS_DIAGONAL_INERTIA_KG_M2[1])
INERTIA_Z = wp.constant(PX4_IRIS_DIAGONAL_INERTIA_KG_M2[2])
GRAVITY_MPS2 = wp.constant(9.80665)
LAUNCH_SURFACE_CENTER_UP_M = wp.constant(0.04)
GROUND_FRICTION_PER_SECOND = wp.constant(8.0)
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
def advance_fleet_and_sample_hil(
    controls: wp.array2d(dtype=wp.float32),
    motor_velocity: wp.array2d(dtype=wp.float32),
    body_indices: wp.array(dtype=wp.int32),
    body_q: wp.array(dtype=wp.transform),
    body_qd: wp.array(dtype=wp.spatial_vector),
    previous_linear_velocity_enu: wp.array2d(dtype=wp.float32),
    packet: wp.array2d(dtype=wp.float32),
    dt: wp.float32,
    origin_latitude_degrees: wp.float32,
    origin_longitude_degrees: wp.float32,
    origin_altitude_m: wp.float32,
    meters_per_degree_latitude: wp.float32,
    meters_per_degree_longitude: wp.float32,
):
    """Advance one native Newton body and emit its complete PX4 HIL sample."""
    vehicle = wp.tid()
    body = body_indices[vehicle]
    transform = body_q[body]
    spatial_velocity = body_qd[body]
    position = wp.transform_get_translation(transform)
    orientation = wp.transform_get_rotation(transform)
    linear_velocity = wp.spatial_top(spatial_velocity)
    angular_velocity = wp.spatial_bottom(spatial_velocity)

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

    velocity_flu = wp.quat_rotate_inv(orientation, linear_velocity)
    force_flu = wp.vec3(
        -DRAG_X * velocity_flu[0],
        -DRAG_Y * velocity_flu[1],
        force_0 + force_1 + force_2 + force_3,
    )
    torque_flu = wp.vec3(
        ROTOR_0_Y * force_0
        + ROTOR_1_Y * force_1
        + ROTOR_2_Y * force_2
        + ROTOR_3_Y * force_3,
        -(
            ROTOR_0_X * force_0
            + ROTOR_1_X * force_1
            + ROTOR_2_X * force_2
            + ROTOR_3_X * force_3
        ),
        YAW_MOMENT_COEFFICIENT
        * (
            ROTOR_0_DIRECTION * motor_velocity[vehicle, 0] * motor_velocity[vehicle, 0]
            + ROTOR_1_DIRECTION * motor_velocity[vehicle, 1] * motor_velocity[vehicle, 1]
            + ROTOR_2_DIRECTION * motor_velocity[vehicle, 2] * motor_velocity[vehicle, 2]
            + ROTOR_3_DIRECTION * motor_velocity[vehicle, 3] * motor_velocity[vehicle, 3]
        ),
    )

    force_world = wp.quat_rotate(orientation, force_flu)
    linear_velocity = linear_velocity + (
        force_world / MASS_KG + wp.vec3(0.0, 0.0, -GRAVITY_MPS2)
    ) * dt
    position = position + linear_velocity * dt

    angular_body = wp.quat_rotate_inv(orientation, angular_velocity)
    inertia_angular = wp.vec3(
        INERTIA_X * angular_body[0],
        INERTIA_Y * angular_body[1],
        INERTIA_Z * angular_body[2],
    )
    torque_body = torque_flu - wp.cross(angular_body, inertia_angular)
    angular_delta = wp.vec3(
        torque_body[0] / INERTIA_X,
        torque_body[1] / INERTIA_Y,
        torque_body[2] / INERTIA_Z,
    ) * dt
    angular_velocity = wp.quat_rotate(orientation, angular_body + angular_delta)
    orientation = wp.normalize(
        orientation + wp.quat(angular_velocity, 0.0) * orientation * 0.5 * dt
    )

    if position[2] < LAUNCH_SURFACE_CENTER_UP_M:
        position = wp.vec3(position[0], position[1], LAUNCH_SURFACE_CENTER_UP_M)
        if linear_velocity[2] < 0.0:
            linear_velocity = wp.vec3(linear_velocity[0], linear_velocity[1], 0.0)
        friction = wp.max(0.0, 1.0 - GROUND_FRICTION_PER_SECOND * dt)
        linear_velocity = wp.vec3(
            linear_velocity[0] * friction,
            linear_velocity[1] * friction,
            linear_velocity[2],
        )
        angular_velocity = angular_velocity * friction

    body_q[body] = wp.transform(position, orientation)
    body_qd[body] = wp.spatial_vector(linear_velocity, angular_velocity)

    previous_velocity = wp.vec3(
        previous_linear_velocity_enu[vehicle, 0],
        previous_linear_velocity_enu[vehicle, 1],
        previous_linear_velocity_enu[vehicle, 2],
    )
    specific_force_enu = (linear_velocity - previous_velocity) / dt - wp.vec3(
        0.0, 0.0, -GRAVITY_MPS2
    )
    acceleration_flu = wp.quat_rotate_inv(orientation, specific_force_enu)
    angular_flu = wp.quat_rotate_inv(orientation, angular_velocity)
    magnetic_flu = wp.quat_rotate_inv(orientation, wp.vec3(0.0, 0.215, -0.427))

    east = position[0]
    north = position[1]
    up = position[2]
    altitude = origin_altitude_m + up
    temperature_kelvin = wp.max(180.0, 288.15 - 0.0065 * altitude)
    pressure_hpa = 1013.25 / wp.pow(288.15 / temperature_kelvin, 5.2561)
    latitude = origin_latitude_degrees + north / meters_per_degree_latitude
    longitude = origin_longitude_degrees + east / meters_per_degree_longitude
    ground_speed = wp.sqrt(
        linear_velocity[0] * linear_velocity[0]
        + linear_velocity[1] * linear_velocity[1]
    )
    course = wp.atan2(linear_velocity[0], linear_velocity[1]) * 57.29577951308232
    if course < 0.0:
        course = course + 360.0

    packet[vehicle, 0] = east
    packet[vehicle, 1] = north
    packet[vehicle, 2] = up
    packet[vehicle, 3] = orientation[0]
    packet[vehicle, 4] = orientation[1]
    packet[vehicle, 5] = orientation[2]
    packet[vehicle, 6] = orientation[3]
    packet[vehicle, 7] = linear_velocity[0]
    packet[vehicle, 8] = linear_velocity[1]
    packet[vehicle, 9] = linear_velocity[2]
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
    packet[vehicle, 25] = linear_velocity[1]
    packet[vehicle, 26] = linear_velocity[0]
    packet[vehicle, 27] = -linear_velocity[2]
    packet[vehicle, 28] = ground_speed
    packet[vehicle, 29] = course

    previous_linear_velocity_enu[vehicle, 0] = linear_velocity[0]
    previous_linear_velocity_enu[vehicle, 1] = linear_velocity[1]
    previous_linear_velocity_enu[vehicle, 2] = linear_velocity[2]
