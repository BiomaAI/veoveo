"""Unit tests for the toolset over the deterministic fake driver — no SUMO."""

from __future__ import annotations

import pytest

from sumo_mcp.sim_driver import FakeSimDriver
from sumo_mcp.tools import (
    OfflineOpParams,
    RerouteVehicleParams,
    RunBatchParams,
    SetSignalPhaseParams,
    SumoToolset,
)


def toolset() -> SumoToolset:
    return SumoToolset(FakeSimDriver(n_vehicles=8, seed=7, congestion_window=(5, 10)))


async def test_query_state_shape() -> None:
    ts = toolset()
    state = await ts.query_state()
    assert state.vehicle_count == 8
    assert len(state.vehicles) == 8
    assert state.mean_speed_mps > 0
    assert {s.id for s in state.signals} == {"tl_center"}


async def test_describe_scenario() -> None:
    info = await toolset().describe_scenario()
    assert info.name == "grid-fake"
    assert "edge_0" in info.edges
    assert info.signals == ["tl_center"]


async def test_determinism_same_seed_same_state() -> None:
    a = await toolset().query_state()
    b = await toolset().query_state()
    assert [v.speed_mps for v in a.vehicles] == [v.speed_mps for v in b.vehicles]
    assert [v.lat for v in a.vehicles] == [v.lat for v in b.vehicles]


async def test_run_batch_detects_scripted_congestion() -> None:
    # Jam window [5,10): stepping through it must drive mean speed below the
    # congestion threshold, deterministically.
    ts = toolset()
    result = await ts.run_batch(RunBatchParams(steps=12))
    assert result.steps_advanced == 12
    assert result.final_sim_time_s == 12.0
    assert result.congestion_detected is True
    assert result.min_mean_speed_mps < SumoToolset.CONGESTION_THRESHOLD_MPS


async def test_run_batch_no_congestion_outside_window() -> None:
    ts = SumoToolset(FakeSimDriver(n_vehicles=8, seed=7, congestion_window=(1000, 1001)))
    result = await ts.run_batch(RunBatchParams(steps=10))
    assert result.congestion_detected is False


async def test_set_signal_phase_and_reroute() -> None:
    ts = toolset()
    ack = await ts.set_signal_phase(SetSignalPhaseParams(signal_id="tl_center", phase=2))
    assert ack.ok
    state = await ts.query_state()
    assert next(s for s in state.signals if s.id == "tl_center").phase == 2

    ack2 = await ts.reroute_vehicle(RerouteVehicleParams(vehicle_id="veh_0", target_edge="edge_3"))
    assert ack2.ok
    state2 = await ts.query_state()
    assert next(v for v in state2.vehicles if v.id == "veh_0").edge == "edge_3"


async def test_actuation_rejects_unknown_targets() -> None:
    ts = toolset()
    with pytest.raises(KeyError):
        await ts.set_signal_phase(SetSignalPhaseParams(signal_id="nope", phase=0))
    with pytest.raises(KeyError):
        await ts.reroute_vehicle(RerouteVehicleParams(vehicle_id="veh_0", target_edge="ghost"))


async def test_offline_op_is_typed() -> None:
    res = await toolset().offline_op("generate_network", OfflineOpParams(kind="grid", seed=3))
    assert res.op == "generate_network"
    assert res.artifact == "generate_network-grid-3.xml"


def test_params_reject_extra_fields() -> None:
    import pydantic

    with pytest.raises(pydantic.ValidationError):
        RunBatchParams(steps=5, bogus=1)  # type: ignore[call-arg]
    with pytest.raises(pydantic.ValidationError):
        RunBatchParams(steps=0)  # gt=0
