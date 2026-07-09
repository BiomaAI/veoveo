"""Typed tool contracts and the toolset logic.

Every tool param and result is a pydantic model with `extra="forbid"`, so
contracts are self-documenting and reject junk. The `SumoToolset` operates on a
`SimDriver` behind an asyncio lock — the single-owner serialization, in-process:
synchronous tools read the latest state, actuation tools apply at the next safe
point, and long ops (run_batch/offline generators) are the task-native ones.
"""

from __future__ import annotations

import asyncio

from pydantic import BaseModel, ConfigDict, Field

from .sim_driver import SimDriver


class _Model(BaseModel):
    model_config = ConfigDict(extra="forbid")


# ---- typed domain values -------------------------------------------------


class Vehicle(_Model):
    id: str
    lat: float
    lon: float
    speed_mps: float
    edge: str


class Signal(_Model):
    id: str
    phase: int


# ---- tool params / results ----------------------------------------------


class QueryStateResult(_Model):
    sim_time_s: float
    vehicle_count: int
    mean_speed_mps: float
    vehicles: list[Vehicle]
    signals: list[Signal]


class DescribeScenarioResult(_Model):
    name: str
    edge_count: int
    signal_count: int
    edges: list[str]  # a bounded sample; edge_count is the true total
    signals: list[str]  # a bounded sample; signal_count is the true total
    origin_lat: float
    origin_lon: float


class SetSignalPhaseParams(_Model):
    signal_id: str
    phase: int = Field(ge=0)


class RerouteVehicleParams(_Model):
    vehicle_id: str
    target_edge: str


class SetEdgeSpeedParams(_Model):
    edge_id: str
    speed_mps: float = Field(ge=0, le=60)


class LaneParams(_Model):
    lane_id: str


class AckResult(_Model):
    ok: bool
    detail: str


class RunBatchParams(_Model):
    steps: int = Field(gt=0, le=100_000)


class RunBatchResult(_Model):
    steps_advanced: int
    final_sim_time_s: float
    final_mean_speed_mps: float
    min_mean_speed_mps: float
    congestion_detected: bool


class OfflineOpParams(_Model):
    """Params for the shell-out offline generators (network/routes/signals)."""

    kind: str = Field(description="grid|spider|osm for network; scenario name otherwise")
    seed: int = 0


class OfflineOpResult(_Model):
    op: str
    kind: str
    artifact: str
    summary: str


class SumoToolset:
    """Serialized access to the one sim driver."""

    CONGESTION_THRESHOLD_MPS = 3.0

    def __init__(self, driver: SimDriver) -> None:
        self._driver = driver
        self._lock = asyncio.Lock()

    @property
    def driver(self) -> SimDriver:
        return self._driver

    # A bounded sample size for on-demand descriptive lists on a dense network.
    SAMPLE = 50

    async def step_once(self):
        """Advance one step and return (vehicles, mean_speed, vehicle_count) under
        the lock — the push loop uses this so stepping never races MCP tool access
        to the single sim owner. Signals are left out of the hot path; they are a
        per-junction read that `query_state` serves on demand."""
        async with self._lock:
            self._driver.step(1)
            return (
                self._driver.vehicles(),
                self._driver.mean_speed(),
                self._driver.vehicle_count(),
            )

    async def query_state(self) -> QueryStateResult:
        async with self._lock:
            vs = self._driver.vehicles()
            sigs = self._driver.signals()
            return QueryStateResult(
                sim_time_s=self._driver.sim_time(),
                vehicle_count=self._driver.vehicle_count(),
                mean_speed_mps=self._driver.mean_speed(),
                vehicles=[
                    Vehicle(id=v.id, lat=v.lat, lon=v.lon, speed_mps=v.speed_mps, edge=v.edge)
                    for v in vs
                ],
                signals=[Signal(id=s.id, phase=s.phase) for s in sigs[: self.SAMPLE]],
            )

    async def describe_scenario(self) -> DescribeScenarioResult:
        async with self._lock:
            info = self._driver.describe()
            return DescribeScenarioResult(
                name=info.name,
                edge_count=len(info.edges),
                signal_count=len(info.signals),
                edges=info.edges[: self.SAMPLE],
                signals=info.signals[: self.SAMPLE],
                origin_lat=info.origin_lat,
                origin_lon=info.origin_lon,
            )

    async def set_signal_phase(self, params: SetSignalPhaseParams) -> AckResult:
        async with self._lock:
            self._driver.set_signal_phase(params.signal_id, params.phase)
            return AckResult(ok=True, detail=f"{params.signal_id} -> phase {params.phase}")

    async def reroute_vehicle(self, params: RerouteVehicleParams) -> AckResult:
        async with self._lock:
            self._driver.reroute_vehicle(params.vehicle_id, params.target_edge)
            return AckResult(ok=True, detail=f"{params.vehicle_id} -> {params.target_edge}")

    async def set_edge_speed(self, params: SetEdgeSpeedParams) -> AckResult:
        async with self._lock:
            self._driver.set_edge_speed(params.edge_id, params.speed_mps)
            return AckResult(ok=True, detail=f"{params.edge_id} -> {params.speed_mps:.1f} m/s")

    async def close_lane(self, params: LaneParams) -> AckResult:
        async with self._lock:
            self._driver.close_lane(params.lane_id)
            return AckResult(ok=True, detail=f"closed {params.lane_id}")

    async def open_lane(self, params: LaneParams) -> AckResult:
        async with self._lock:
            self._driver.open_lane(params.lane_id)
            return AckResult(ok=True, detail=f"opened {params.lane_id}")

    async def run_batch(self, params: RunBatchParams) -> RunBatchResult:
        """Fast-forward the sim, tracking the worst congestion seen — the long op
        the agent detaches from and wakes on."""
        min_mean = float("inf")
        async with self._lock:
            for _ in range(params.steps):
                self._driver.step(1)
                min_mean = min(min_mean, self._driver.mean_speed())
                # Yield so status notifications and other awaits can interleave.
                await asyncio.sleep(0)
            final_mean = self._driver.mean_speed()
            return RunBatchResult(
                steps_advanced=params.steps,
                final_sim_time_s=self._driver.sim_time(),
                final_mean_speed_mps=final_mean,
                min_mean_speed_mps=min_mean,
                congestion_detected=min_mean < self.CONGESTION_THRESHOLD_MPS,
            )

    async def offline_op(self, op: str, params: OfflineOpParams) -> OfflineOpResult:
        """A shell-out generator. Against a SUMO install it invokes the real CLI
        (netgenerate / duarouter / tlsCoordinator) in a scratch workspace and
        reports a real metric; without SUMO (tests/host) it returns a
        deterministic typed stub of the same shape. Either way the task lifecycle
        is what the agent detaches from and wakes on."""
        artifact = f"{op}-{params.kind}-{params.seed}.xml"
        try:
            summary = await asyncio.to_thread(_run_offline_real, op, params)
            return OfflineOpResult(op=op, kind=params.kind, artifact=artifact, summary=summary)
        except Exception as exc:  # missing binaries or a tool error → deterministic stub
            return OfflineOpResult(
                op=op, kind=params.kind, artifact=artifact, summary=f"stub ({exc.__class__.__name__})"
            )


# --- real offline generators (invoked only when a SUMO install is present) ----


def _count_tag(path: str, tag: str) -> int:
    with open(path) as f:
        return f.read().count(tag)


def _edge_ids(net_path: str) -> list[str]:
    import re

    with open(net_path) as f:
        return [e for e in re.findall(r'<edge id="([^"]+)"', f.read()) if not e.startswith(":")]


def _write_trips(path: str, edges: list[str], seed: int, count: int) -> None:
    import random

    rng = random.Random(seed)
    rows = ["<routes>"]
    for i in range(count):
        rows.append(f'  <trip id="t{i}" depart="{i}" from="{rng.choice(edges)}" to="{rng.choice(edges)}"/>')
    rows.append("</routes>")
    with open(path, "w") as f:
        f.write("\n".join(rows))


def _run_offline_real(op: str, params: OfflineOpParams) -> str:
    """Invoke the genuine SUMO CLI in a scratch workspace and return a real
    metric. Raises if the binaries are absent — the caller then falls back to a
    deterministic stub, so this never has to be guarded at the call site."""
    import os
    import subprocess
    import tempfile

    from sumolib import checkBinary  # raises without an eclipse-sumo install

    with tempfile.TemporaryDirectory() as d:
        net = os.path.join(d, "net.xml")
        n = 4 + (params.seed % 4)
        subprocess.run(
            [checkBinary("netgenerate"), "--grid", "--grid.number", str(n),
             "--tls.guess", "true", "--tls.guess.threshold", "0", "-o", net],
            check=True, capture_output=True, cwd=d, timeout=120,
        )
        if op == "generate_network":
            return f"netgenerate {params.kind} {n}x{n}: {_count_tag(net, '<edge ')} edges"
        if op == "optimize_signals":
            out = os.path.join(d, "coord.xml")
            subprocess.run(
                [checkBinary("tlsCoordinator"), "-n", net, "-o", out],
                check=True, capture_output=True, cwd=d, timeout=120,
            )
            return f"tlsCoordinator: {_count_tag(out, '<tlLogic')} coordinated programs"
        if op == "compute_routes":
            trips = os.path.join(d, "trips.xml")
            routes = os.path.join(d, "routes.xml")
            _write_trips(trips, _edge_ids(net), params.seed, count=50)
            subprocess.run(
                [checkBinary("duarouter"), "-n", net, "--route-files", trips,
                 "-o", routes, "--ignore-errors", "true"],
                check=True, capture_output=True, cwd=d, timeout=120,
            )
            return f"duarouter: {_count_tag(routes, '<vehicle')} routed vehicles"
        raise ValueError(f"unknown offline op {op}")
