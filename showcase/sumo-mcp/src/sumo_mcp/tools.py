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
    edges: list[str]
    signals: list[str]
    origin_lat: float
    origin_lon: float


class SetSignalPhaseParams(_Model):
    signal_id: str
    phase: int = Field(ge=0)


class RerouteVehicleParams(_Model):
    vehicle_id: str
    target_edge: str


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

    async def query_state(self) -> QueryStateResult:
        async with self._lock:
            vs = self._driver.vehicles()
            sigs = self._driver.signals()
            return QueryStateResult(
                sim_time_s=self._driver.sim_time(),
                vehicle_count=len(vs),
                mean_speed_mps=self._driver.mean_speed(),
                vehicles=[
                    Vehicle(id=v.id, lat=v.lat, lon=v.lon, speed_mps=v.speed_mps, edge=v.edge)
                    for v in vs
                ],
                signals=[Signal(id=s.id, phase=s.phase) for s in sigs],
            )

    async def describe_scenario(self) -> DescribeScenarioResult:
        async with self._lock:
            info = self._driver.describe()
            return DescribeScenarioResult(
                name=info.name,
                edges=info.edges,
                signals=info.signals,
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
        """A shell-out generator (network/routes/signals). In the fake world it
        returns a deterministic typed summary; against real SUMO it invokes the
        corresponding CLI (netgenerate/duarouter/tlsCoordinator)."""
        # Simulate meaningful work so the task lifecycle is observable.
        await asyncio.sleep(0)
        artifact = f"{op}-{params.kind}-{params.seed}.xml"
        return OfflineOpResult(
            op=op,
            kind=params.kind,
            artifact=artifact,
            summary=f"{op} produced {artifact}",
        )
