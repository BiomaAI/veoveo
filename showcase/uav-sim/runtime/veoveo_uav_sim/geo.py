from __future__ import annotations

import math


WGS84_A = 6_378_137.0
WGS84_F = 1.0 / 298.257_223_563
WGS84_E2 = WGS84_F * (2.0 - WGS84_F)


def horizontal_distance_m(
    latitude_a: float,
    longitude_a: float,
    latitude_b: float,
    longitude_b: float,
) -> float:
    latitude_a_radians = math.radians(latitude_a)
    latitude_b_radians = math.radians(latitude_b)
    latitude_delta = latitude_b_radians - latitude_a_radians
    longitude_delta = math.radians(longitude_b - longitude_a)
    haversine = (
        math.sin(latitude_delta / 2.0) ** 2
        + math.cos(latitude_a_radians)
        * math.cos(latitude_b_radians)
        * math.sin(longitude_delta / 2.0) ** 2
    )
    return 2.0 * WGS84_A * math.asin(min(1.0, math.sqrt(haversine)))


def geodetic_to_ecef(latitude_degrees: float, longitude_degrees: float, height_m: float) -> tuple[float, float, float]:
    latitude = math.radians(latitude_degrees)
    longitude = math.radians(longitude_degrees)
    sin_latitude = math.sin(latitude)
    cos_latitude = math.cos(latitude)
    radius = WGS84_A / math.sqrt(1.0 - WGS84_E2 * sin_latitude * sin_latitude)
    return (
        (radius + height_m) * cos_latitude * math.cos(longitude),
        (radius + height_m) * cos_latitude * math.sin(longitude),
        (radius * (1.0 - WGS84_E2) + height_m) * sin_latitude,
    )


def ecef_to_geodetic(x: float, y: float, z: float) -> tuple[float, float, float]:
    longitude = math.atan2(y, x)
    p = math.hypot(x, y)
    latitude = math.atan2(z, p * (1.0 - WGS84_E2))
    height = 0.0
    for _ in range(8):
        sin_latitude = math.sin(latitude)
        radius = WGS84_A / math.sqrt(1.0 - WGS84_E2 * sin_latitude * sin_latitude)
        height = p / max(math.cos(latitude), 1e-15) - radius
        latitude = math.atan2(z, p * (1.0 - WGS84_E2 * radius / (radius + height)))
    return math.degrees(latitude), math.degrees(longitude), height


def enu_to_geodetic(
    east_m: float,
    north_m: float,
    up_m: float,
    origin_latitude_degrees: float,
    origin_longitude_degrees: float,
    origin_height_m: float,
) -> tuple[float, float, float]:
    x0, y0, z0 = geodetic_to_ecef(
        origin_latitude_degrees, origin_longitude_degrees, origin_height_m
    )
    latitude = math.radians(origin_latitude_degrees)
    longitude = math.radians(origin_longitude_degrees)
    sin_latitude, cos_latitude = math.sin(latitude), math.cos(latitude)
    sin_longitude, cos_longitude = math.sin(longitude), math.cos(longitude)
    x = x0 - sin_longitude * east_m - sin_latitude * cos_longitude * north_m + cos_latitude * cos_longitude * up_m
    y = y0 + cos_longitude * east_m - sin_latitude * sin_longitude * north_m + cos_latitude * sin_longitude * up_m
    z = z0 + cos_latitude * north_m + sin_latitude * up_m
    return ecef_to_geodetic(x, y, z)
