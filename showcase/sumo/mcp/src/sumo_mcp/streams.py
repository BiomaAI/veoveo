"""The push path: SUMO world state → typed Rerun streams → the Recording Hub.

The sim owner publishes each step as world-model components under
`/world/sumo/**` — into the hub's gRPC proxy. This is the "tiny simulator
pushing to rerun" spine: SUMO is one more producer; the hub never pulls.

Two complementary views come out of one frame:
  * the geographic map — cars as GeoPoints on real streets, each with a short
    chevron (GeoLineStrings) showing which way it faces;
  * a 3D scene — cars as oriented Boxes3D sized to their real footprint (a bus
    is long and tall), over the road network drawn once as static line strips.
Speed is colour-coded on a red→amber→green ramp weighted toward the jam end, so
congestion reads at a glance in both.

The rerun SDK is an optional dependency, imported lazily, so the MCP server and
its tests work without it.
"""

from __future__ import annotations

import math
from collections.abc import Sequence

from .sim_driver import VehicleState

WORLD_TIMELINE = "tick"

# Free-flow reference for colouring: at/above this a car is "green", at 0 "red".
_FREE_FLOW_MPS = 14.0
_M_PER_DEG_LAT = 111_320.0

# The traffic-light ramp, as stops on a red → amber → green scale.
_JAM = (205, 35, 45)
_SLOW = (240, 170, 35)
_FREE = (45, 200, 90)


def _lerp(c0: tuple[int, int, int], c1: tuple[int, int, int], u: float) -> list[int]:
    return [round(a + (b - a) * u) for a, b in zip(c0, c1)]


def _speed_color(speed_mps: float) -> list[int]:
    """Congested (red) → free-flowing (green) on a red→amber→green ramp. The
    gamma bias (t**1.4) spends more of the scale on low speeds, so stopped and
    crawling traffic stays vividly red instead of washing to amber."""
    t = max(0.0, min(1.0, speed_mps / _FREE_FLOW_MPS)) ** 1.4
    if t < 0.5:
        return _lerp(_JAM, _SLOW, t / 0.5)
    return _lerp(_SLOW, _FREE, (t - 0.5) / 0.5)


def _chevron_latlon(v: VehicleState) -> list[list[float]]:
    """A small 3-point arrowhead (back-left → tip → back-right) pointing along
    the vehicle's heading, in lat/lon — the map's facing hint. Heading is SUMO's
    convention: 0 = north, clockwise."""
    h = math.radians(v.heading_deg)
    # Unit vectors in (east, north) metres.
    fwd = (math.sin(h), math.cos(h))
    right = (math.cos(h), -math.sin(h))
    ahead, back, half_w = 10.0, 4.0, 3.5
    corners = (
        (-back * fwd[0] - half_w * right[0], -back * fwd[1] - half_w * right[1]),
        (ahead * fwd[0], ahead * fwd[1]),
        (-back * fwd[0] + half_w * right[0], -back * fwd[1] + half_w * right[1]),
    )
    m_per_deg_lon = _M_PER_DEG_LAT * math.cos(math.radians(v.lat)) or _M_PER_DEG_LAT
    return [[v.lat + n / _M_PER_DEG_LAT, v.lon + e / m_per_deg_lon] for e, n in corners]


class RerunPublisher:
    """Publishes SUMO state into the hub as one Rerun recording (one session)."""

    def __init__(
        self,
        proxy_url: str,
        application_id: str = "veoveo-sumo",
        recording: str = "sumo-run",
    ) -> None:
        import rerun as rr  # lazy: only needed on the push path

        self._rr = rr
        self._stream = rr.RecordingStream(application_id, recording_id=recording)
        rr.connect_grpc(proxy_url, recording=self._stream)

    def publish_network(self, strips: Sequence[Sequence[tuple[float, float]]]) -> None:
        """Log the road network once as static 3D line strips — the ground plan
        the vehicle boxes drive over. Static, so it persists across every frame
        without being re-sent."""
        if not strips:
            return
        rr = self._rr
        rr.log(
            "/world/sumo/network",
            rr.LineStrips3D(
                strips=[[[x, y, 0.0] for (x, y) in strip] for strip in strips],
                colors=[80, 84, 96],
                radii=[0.8],  # ~0.8 m, a road-like ribbon
            ),
            static=True,
            recording=self._stream,
        )

    def publish(
        self,
        step: int,
        vehicles: Sequence[VehicleState],
        mean_speed: float,
        vehicle_count: int,
    ) -> None:
        rr = self._rr
        rr.set_time(WORLD_TIMELINE, sequence=step, recording=self._stream)
        colors = [_speed_color(v.speed_mps) for v in vehicles]

        # Map: every vehicle as one geo point cloud, coloured by speed — one log
        # call per frame, so a dense city stays smooth and reads as a live map.
        rr.log(
            "/world/sumo/vehicles",
            rr.GeoPoints(
                lat_lon=[[v.lat, v.lon] for v in vehicles],
                colors=colors,
                radii=[-4.0],  # 4 UI points, so cars stay visible at any zoom
            ),
            recording=self._stream,
        )
        # Map: a facing chevron per vehicle, batched into one GeoLineStrings.
        rr.log(
            "/world/sumo/heading",
            rr.GeoLineStrings(
                lat_lon=[_chevron_latlon(v) for v in vehicles],
                colors=colors,
                radii=[-1.5],
            ),
            recording=self._stream,
        )
        # 3D: oriented boxes sized to each vehicle's real footprint, rotated by
        # heading (yaw about up), coloured by speed — one batched log.
        rr.log(
            "/world/sumo/vehicles3d",
            rr.Boxes3D(
                centers=[[v.x, v.y, v.height_m / 2.0] for v in vehicles],
                half_sizes=[
                    [v.length_m / 2.0, v.width_m / 2.0, v.height_m / 2.0] for v in vehicles
                ],
                rotation_axis_angles=[
                    rr.RotationAxisAngle([0.0, 0.0, 1.0], rr.Angle(deg=90.0 - v.heading_deg))
                    for v in vehicles
                ],
                colors=colors,
                fill_mode="solid",
            ),
            recording=self._stream,
        )
        rr.log("/world/sumo/mean_speed", rr.Scalars([mean_speed]), recording=self._stream)
        rr.log(
            "/world/sumo/vehicle_count",
            rr.Scalars([float(vehicle_count)]),
            recording=self._stream,
        )

    def flush(self) -> None:
        self._stream.flush()
