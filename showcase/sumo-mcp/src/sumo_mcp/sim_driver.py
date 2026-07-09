"""The device driver for the SUMO world.

TraCI is single-client and stepping is serialized, so exactly one owner may hold
the connection. This module defines that owner as a `SimDriver` protocol with two
implementations: `FakeSimDriver` (deterministic, no SUMO — the unit tests' world)
and `TraciSimDriver` (the real connection, imported lazily so the package works
without SUMO installed). The server talks only to the protocol, never to traci
directly, so tools are testable and the concurrency model is one clear owner.
"""

from __future__ import annotations

import math
from dataclasses import dataclass, field
from typing import Protocol, runtime_checkable


@dataclass(frozen=True)
class VehicleState:
    id: str
    lat: float
    lon: float
    speed_mps: float
    edge: str


@dataclass(frozen=True)
class SignalState:
    id: str
    phase: int


@dataclass(frozen=True)
class ScenarioInfo:
    name: str
    edges: list[str]
    signals: list[str]
    origin_lat: float
    origin_lon: float


@runtime_checkable
class SimDriver(Protocol):
    """The single owner of the simulation. All methods run on the sim thread."""

    def describe(self) -> ScenarioInfo: ...
    def sim_time(self) -> float: ...
    def step(self, n: int = 1) -> None: ...
    def vehicles(self) -> list[VehicleState]: ...
    def vehicle_count(self) -> int: ...
    def signals(self) -> list[SignalState]: ...
    def mean_speed(self) -> float: ...
    def set_signal_phase(self, signal_id: str, phase: int) -> None: ...
    def reroute_vehicle(self, vehicle_id: str, target_edge: str) -> None: ...
    def set_edge_speed(self, edge_id: str, speed_mps: float) -> None: ...
    def close_lane(self, lane_id: str) -> None: ...
    def open_lane(self, lane_id: str) -> None: ...
    def close(self) -> None: ...


# A stable pseudo-random unit value from (seed, tick) — no global RNG, so the
# fake world is identical across runs and machines.
def _seeded_unit(seed: int, tick: int) -> float:
    z = (seed + 0x9E3779B97F4A7C15) * (tick + 1) & 0xFFFFFFFFFFFFFFFF
    z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & 0xFFFFFFFFFFFFFFFF
    z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & 0xFFFFFFFFFFFFFFFF
    z ^= z >> 31
    return (z >> 11) / float(1 << 53)


@dataclass
class FakeSimDriver:
    """A deterministic traffic world with no SUMO dependency.

    Vehicles orbit a grid origin at speeds that dip during a scripted congestion
    window, so `mean_speed()` crosses a threshold predictably — the event plane's
    congestion condition is testable without a simulator. Everything is a pure
    function of (seed, step), so tests assert exact values.
    """

    name: str = "grid-fake"
    n_vehicles: int = 12
    seed: int = 1
    dt_s: float = 1.0
    origin_lat: float = 47.3769
    origin_lon: float = 8.5417
    radius_m: float = 200.0
    # Steps [start, end) where mean speed collapses (a jam), for the event plane.
    congestion_window: tuple[int, int] = (40, 60)
    _step: int = field(default=0, init=False)
    _signal_phase: dict[str, int] = field(default_factory=dict, init=False)
    _rerouted: dict[str, str] = field(default_factory=dict, init=False)
    _edge_speeds: dict[str, float] = field(default_factory=dict, init=False)
    _closed_lanes: set[str] = field(default_factory=set, init=False)

    _M_PER_DEG_LAT = 111_320.0

    def _edges(self) -> list[str]:
        return [f"edge_{i}" for i in range(4)]

    def _signal_ids(self) -> list[str]:
        return ["tl_center"]

    def describe(self) -> ScenarioInfo:
        return ScenarioInfo(
            name=self.name,
            edges=self._edges(),
            signals=self._signal_ids(),
            origin_lat=self.origin_lat,
            origin_lon=self.origin_lon,
        )

    def sim_time(self) -> float:
        return self._step * self.dt_s

    def step(self, n: int = 1) -> None:
        if n < 0:
            raise ValueError("step count must be non-negative")
        self._step += n

    def _in_jam(self) -> bool:
        lo, hi = self.congestion_window
        return lo <= self._step < hi

    def _vehicle_speed(self, i: int) -> float:
        base = 12.0 + 4.0 * math.sin(self.sim_time() * 0.3 + i)
        if self._in_jam():
            base *= 0.15  # gridlock
        # Deterministic per-vehicle jitter.
        return max(0.0, base + (_seeded_unit(self.seed, self._step * 97 + i) - 0.5))

    def vehicles(self) -> list[VehicleState]:
        m_per_deg_lon = self._M_PER_DEG_LAT * math.cos(math.radians(self.origin_lat))
        out: list[VehicleState] = []
        for i in range(self.n_vehicles):
            theta = (math.tau * i / self.n_vehicles) + self.sim_time() * 0.05
            dx = self.radius_m * math.cos(theta)
            dy = self.radius_m * math.sin(theta)
            out.append(
                VehicleState(
                    id=f"veh_{i}",
                    lat=self.origin_lat + dy / self._M_PER_DEG_LAT,
                    lon=self.origin_lon + dx / m_per_deg_lon,
                    speed_mps=self._vehicle_speed(i),
                    edge=self._rerouted.get(f"veh_{i}", f"edge_{i % 4}"),
                )
            )
        return out

    def vehicle_count(self) -> int:
        return self.n_vehicles

    def signals(self) -> list[SignalState]:
        return [
            SignalState(id=s, phase=self._signal_phase.get(s, 0))
            for s in self._signal_ids()
        ]

    def mean_speed(self) -> float:
        vs = self.vehicles()
        return sum(v.speed_mps for v in vs) / len(vs) if vs else 0.0

    def set_signal_phase(self, signal_id: str, phase: int) -> None:
        if signal_id not in self._signal_ids():
            raise KeyError(f"unknown signal {signal_id}")
        if phase < 0:
            raise ValueError("phase must be non-negative")
        self._signal_phase[signal_id] = phase

    def reroute_vehicle(self, vehicle_id: str, target_edge: str) -> None:
        if target_edge not in self._edges():
            raise KeyError(f"unknown edge {target_edge}")
        self._rerouted[vehicle_id] = target_edge

    def set_edge_speed(self, edge_id: str, speed_mps: float) -> None:
        if edge_id not in self._edges():
            raise KeyError(f"unknown edge {edge_id}")
        if speed_mps < 0:
            raise ValueError("speed must be non-negative")
        self._edge_speeds[edge_id] = speed_mps

    def close_lane(self, lane_id: str) -> None:
        if lane_id.rsplit("_", 1)[0] not in self._edges():
            raise KeyError(f"unknown lane {lane_id}")
        self._closed_lanes.add(lane_id)

    def open_lane(self, lane_id: str) -> None:
        if lane_id.rsplit("_", 1)[0] not in self._edges():
            raise KeyError(f"unknown lane {lane_id}")
        self._closed_lanes.discard(lane_id)

    def close(self) -> None:  # nothing to release
        return None


class TraciSimDriver:
    """The real connection to a running SUMO (started with `--remote-port`).

    `traci` is imported lazily so the package (and its tests) work without SUMO
    installed. This owns the single TraCI connection; the server serializes all
    access to it, so stepping is never interleaved with reads or actuation.
    """

    def __init__(
        self,
        host: str = "127.0.0.1",
        port: int = 8813,
        name: str = "sumo",
        max_vehicles: int = 800,
        connect_retries: int = 180,
        origin_lat: float = 52.5200,
        origin_lon: float = 13.4050,
    ) -> None:
        import traci  # lazy: only on the live path
        from traci import constants as tc

        self._traci = traci
        self._tc = tc
        # A large scenario (LuST) can take a while to load before the TraCI port
        # opens — especially under emulation — so retry generously.
        traci.init(port=port, host=host, numRetries=connect_retries)
        self._name = name
        # Bound the per-frame vehicle set so reads and Rerun frames stay light on
        # a dense city network; aggregates (count, mean speed) still cover all.
        self._max_vehicles = max_vehicles
        self._sub_vars = [tc.VAR_POSITION, tc.VAR_SPEED, tc.VAR_ROAD_ID]
        # Fallback (no network projection): equirectangular about a fixed origin.
        self._lat0 = origin_lat
        self._lon0 = origin_lon
        self._m_per_deg_lat = 111_320.0
        self._m_per_deg_lon = 111_320.0 * math.cos(math.radians(origin_lat))
        self._geo_affine: tuple[float, float, float, float, float, float] | None = None
        self._calibrate_geo()
        # Subscribe any vehicles already present at connect time.
        for vid in traci.vehicle.getIDList():
            traci.vehicle.subscribe(vid, self._sub_vars)

    def _calibrate_geo(self) -> None:
        """Derive a cartesian→lon/lat map once, from the network's own geo
        reference. A real projection (LuST, any OSM network) makes vehicles land
        on the true streets; a projection-less network (a bare grid) falls back
        to the fixed-origin equirectangular map."""
        t = self._traci
        try:
            (xmin, ymin), (xmax, ymax) = t.simulation.getNetBoundary()
            lon_sw, lat_sw = t.simulation.convertGeo(xmin, ymin)
            lon_ne, lat_ne = t.simulation.convertGeo(xmax, ymax)
            # A projection-less net returns the input unchanged (metres as lon/lat)
            # or values outside the geographic range — reject those.
            plausible = (
                abs(lat_sw) <= 90
                and abs(lon_sw) <= 180
                and (abs(lon_sw - xmin) > 1e-6 or abs(lat_sw - ymin) > 1e-6)
                and xmax > xmin
                and ymax > ymin
            )
            if plausible:
                self._geo_affine = (
                    xmin,
                    ymin,
                    lon_sw,
                    lat_sw,
                    (lon_ne - lon_sw) / (xmax - xmin),
                    (lat_ne - lat_sw) / (ymax - ymin),
                )
        except Exception:
            self._geo_affine = None

    def _to_geo(self, x: float, y: float) -> tuple[float, float]:
        """Cartesian metres (SUMO x/y) → (lat, lon)."""
        if self._geo_affine is not None:
            x0, y0, lon0, lat0, sx, sy = self._geo_affine
            return lat0 + (y - y0) * sy, lon0 + (x - x0) * sx
        return self._lat0 + y / self._m_per_deg_lat, self._lon0 + x / self._m_per_deg_lon

    def describe(self) -> ScenarioInfo:
        t = self._traci
        edges = [e for e in t.edge.getIDList() if not e.startswith(":")]
        signals = list(t.trafficlight.getIDList())
        lat0, lon0 = self._to_geo(0.0, 0.0)
        return ScenarioInfo(
            name=self._name,
            edges=edges,
            signals=signals,
            origin_lat=lat0,
            origin_lon=lon0,
        )

    def sim_time(self) -> float:
        return float(self._traci.simulation.getTime())

    def step(self, n: int = 1) -> None:
        if n < 0:
            raise ValueError("step count must be non-negative")
        t = self._traci
        for _ in range(n):
            t.simulationStep()
            # Subscribe newly departed vehicles so the next read is a single
            # round-trip (getAllSubscriptionResults) instead of per-vehicle calls.
            for vid in t.simulation.getDepartedIDList():
                t.vehicle.subscribe(vid, self._sub_vars)

    def _results(self) -> dict:
        return self._traci.vehicle.getAllSubscriptionResults()

    def vehicles(self) -> list[VehicleState]:
        tc = self._tc
        out: list[VehicleState] = []
        for vid, sub in self._results().items():
            x, y = sub[tc.VAR_POSITION]
            lat, lon = self._to_geo(x, y)
            out.append(
                VehicleState(
                    id=vid,
                    lat=lat,
                    lon=lon,
                    speed_mps=float(sub[tc.VAR_SPEED]),
                    edge=sub[tc.VAR_ROAD_ID],
                )
            )
            if len(out) >= self._max_vehicles:
                break
        return out

    def vehicle_count(self) -> int:
        return int(self._traci.vehicle.getIDCount())

    def signals(self) -> list[SignalState]:
        t = self._traci
        return [SignalState(id=s, phase=int(t.trafficlight.getPhase(s))) for s in t.trafficlight.getIDList()]

    def mean_speed(self) -> float:
        tc = self._tc
        speeds = [float(s[tc.VAR_SPEED]) for s in self._results().values()]
        return sum(speeds) / len(speeds) if speeds else 0.0

    def set_signal_phase(self, signal_id: str, phase: int) -> None:
        self._traci.trafficlight.setPhase(signal_id, phase)

    def reroute_vehicle(self, vehicle_id: str, target_edge: str) -> None:
        self._traci.vehicle.changeTarget(vehicle_id, target_edge)

    def set_edge_speed(self, edge_id: str, speed_mps: float) -> None:
        self._traci.edge.setMaxSpeed(edge_id, speed_mps)

    def close_lane(self, lane_id: str) -> None:
        # Disallow every vehicle class → the lane is shut, an incident.
        self._traci.lane.setDisallowed(lane_id, ["all"])

    def open_lane(self, lane_id: str) -> None:
        self._traci.lane.setAllowed(lane_id, ["all"])

    def close(self) -> None:
        with __import__("contextlib").suppress(Exception):
            self._traci.close()
