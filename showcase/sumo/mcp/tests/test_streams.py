"""Unit tests for the visualization path — colour ramp, facing chevron, and the
new vehicle/geometry fields — all pure, no SUMO and no rerun SDK needed."""

from __future__ import annotations

import math

from sumo_mcp.sim_driver import FakeSimDriver, VehicleState
from sumo_mcp.streams import _chevron_latlon, _speed_color
from sumo_mcp.tools import SumoToolset


def test_speed_color_ramps_red_to_green() -> None:
    stopped = _speed_color(0.0)
    free = _speed_color(20.0)  # above free-flow → clamped green
    assert all(0 <= c <= 255 for c in stopped + free)
    assert stopped[0] > free[0]  # more red when stopped
    assert free[1] > stopped[1]  # more green when free-flowing


def test_speed_color_bias_keeps_crawl_red() -> None:
    # Half of free-flow (7 of 14 m/s) would sit exactly at amber on a linear
    # ramp. The gamma bias keeps it on the red side — its green channel has not
    # yet reached full amber (170) — so crawling traffic still reads as jammed.
    crawl = _speed_color(7.0)
    assert crawl[1] < 170  # greener means faster; still below amber
    # Green channel rises monotonically with speed across the ramp.
    greens = [_speed_color(s)[1] for s in (0.0, 4.0, 8.0, 12.0, 16.0)]
    assert greens == sorted(greens)


def test_chevron_points_along_heading() -> None:
    here = dict(id="v", lat=49.6, lon=6.1, speed_mps=5.0, edge="e")
    north = _chevron_latlon(VehicleState(**here, heading_deg=0.0))
    east = _chevron_latlon(VehicleState(**here, heading_deg=90.0))
    assert len(north) == 3 and len(east) == 3
    # tip is corner index 1; heading north pushes it north (lat up), east pushes
    # it east (lon up).
    assert north[1][0] > 49.6 and math.isclose(north[1][1], 6.1, abs_tol=1e-6)
    assert east[1][1] > 6.1 and math.isclose(east[1][0], 49.6, abs_tol=1e-6)


def test_fake_driver_carries_heading_and_footprint() -> None:
    vs = FakeSimDriver(n_vehicles=10, seed=3).vehicles()
    assert vs, "expected vehicles"
    assert all(0.0 <= v.heading_deg < 360.0 for v in vs)
    buses = [v for v in vs if v.vclass == "bus"]
    cars = [v for v in vs if v.vclass == "passenger"]
    assert buses and cars
    assert min(b.length_m for b in buses) > max(c.length_m for c in cars)


def test_fake_network_geometry_is_polylines() -> None:
    strips = FakeSimDriver().network_geometry()
    assert strips and all(len(s) >= 2 for s in strips)
    assert all(len(pt) == 2 for s in strips for pt in s)


async def test_toolset_network_geometry_under_lock() -> None:
    strips = await SumoToolset(FakeSimDriver()).network_geometry()
    assert strips and all(len(s) >= 2 for s in strips)
