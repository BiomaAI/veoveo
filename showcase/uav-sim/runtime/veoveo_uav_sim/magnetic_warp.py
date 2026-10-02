"""The pinned PX4 earth-field model evaluated at each vehicle's GPS position on CUDA."""
from __future__ import annotations

import warp as wp

from . import _magnetic_tables as tables


DECLINATION_SCALE = wp.constant(tables.WMM_DECLINATION_SCALE_TO_DEGREES)
INCLINATION_SCALE = wp.constant(tables.WMM_INCLINATION_SCALE_TO_DEGREES)
INTENSITY_SCALE = wp.constant(tables.WMM_TOTALINTENSITY_SCALE_TO_NANOTESLA * 1.0e-5)


def upload_magnetic_tables(device: wp.Device) -> wp.array:
    if not device.is_cuda:
        raise ValueError("the magnetic sensor requires a hardware CUDA device")
    return wp.array(
        [tables.DECLINATION, tables.INCLINATION, tables.TOTALINTENSITY],
        dtype=wp.float32, device=device,
    )


@wp.func
def magnetic_field_enu_gauss(
    model: wp.array3d(dtype=wp.float32), latitude: float, longitude: float,
) -> wp.vec3:
    # PX4's geo_mag_declination.cpp clamps latitude, wraps longitude once,
    # and interpolates within the final cell at the upper table bounds.
    latitude = wp.clamp(latitude, -90.0, 90.0)
    if longitude > 180.0:
        longitude -= 360.0
    if longitude < -180.0:
        longitude += 360.0
    min_lat = wp.clamp(wp.floor(latitude / 10.0) * 10.0, -90.0, 80.0)
    min_lon = wp.clamp(wp.floor(longitude / 10.0) * 10.0, -180.0, 170.0)
    row = int((min_lat + 90.0) / 10.0)
    col = int((min_lon + 180.0) / 10.0)
    north = wp.clamp((latitude - min_lat) / 10.0, 0.0, 1.0)
    east = wp.clamp((longitude - min_lon) / 10.0, 0.0, 1.0)
    values = wp.vec3(0.0)
    for index in range(3):
        southwest = model[index, row, col]
        southeast = model[index, row, col + 1]
        northwest = model[index, row + 1, col]
        northeast = model[index, row + 1, col + 1]
        south = east * (southeast - southwest) + southwest
        north_edge = east * (northeast - northwest) + northwest
        values[index] = north * (north_edge - south) + south
    declination = wp.radians(values[0] * DECLINATION_SCALE)
    inclination = wp.radians(values[1] * INCLINATION_SCALE)
    intensity = values[2] * INTENSITY_SCALE
    horizontal = intensity * wp.cos(inclination)
    return wp.vec3(
        horizontal * wp.sin(declination),
        horizontal * wp.cos(declination),
        -intensity * wp.sin(inclination),
    )
