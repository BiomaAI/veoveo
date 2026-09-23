from __future__ import annotations

import asyncio

import pytest

from veoveo_mcp.contract import CONTRACT_REVISION, ComplianceStatus, parse_compliance

from anonymous_simulation_mcp.config import Config
from anonymous_simulation_mcp.contract import (
    CloseLiveViewRequest,
    OpenLiveViewRequest,
    RenewLiveViewRequest,
    ViewLifecycle,
)
from anonymous_simulation_mcp.mcp_server import (
    CONTRACT_DECLARATION,
    DOCS_INDEX,
    LLMS_TXT,
    SERVER_DOCS,
)
from anonymous_simulation_mcp.runtime import (
    CAMERA_ID,
    SESSION_ID,
    STREAM_PRODUCT_ID,
    FixtureRuntime,
)


def _config() -> Config:
    return Config(
        port=8812,
        allowed_hosts=("anonymous-simulation-mcp:8812",),
        internal_trust_jwks='{"keys":[]}',
        public_stream_url="ws://127.0.0.1:8812/anonymous-simulation/live",
        authorization_seconds=3_600,
    )


def _open(instance: str) -> OpenLiveViewRequest:
    return OpenLiveViewRequest(
        session_id=SESSION_ID,
        camera_id=CAMERA_ID,
        viewer_instance_id=instance,
    )


@pytest.mark.asyncio
async def test_twenty_five_viewers_share_one_continuous_product() -> None:
    runtime = FixtureRuntime(_config())
    opened = [
        await runtime.open(
            f"actor-{index}",
            "group:operators",
            _open(f"browser-{index}"),
        )
        for index in range(25)
    ]
    state = await runtime.fixture_state()

    assert len({view.stream.live_view_id for view in opened}) == 25
    assert len({view.access_token for view in opened}) == 25
    assert {view.stream.stream_product_id for view in opened} == {STREAM_PRODUCT_ID}
    assert len(state.stream_products) == 1
    assert state.stream_products[0].camera_regions[0].camera_id == CAMERA_ID
    assert state.stream_products[0].coded_width_px == 1280
    assert state.stream_products[0].active_viewers == 25
    assert state.stream_products[0].nvenc_sessions == 1


@pytest.mark.asyncio
async def test_token_rotation_and_owner_isolation() -> None:
    runtime = FixtureRuntime(_config())
    first = await runtime.open("actor-a", "group:operators", _open("browser-a"))
    request = RenewLiveViewRequest(
        session_id=SESSION_ID,
        live_view_id=first.stream.live_view_id,
        viewer_instance_id="browser-a",
    )
    renewed = await runtime.renew("actor-a", "group:operators", request)
    assert renewed.access_token != first.access_token
    with pytest.raises(ValueError, match="stream authorization"):
        await runtime.authorize_stream(first.stream.live_view_id, first.access_token)
    authorized = await runtime.authorize_stream(
        renewed.stream.live_view_id, renewed.access_token
    )
    assert authorized.lifecycle is ViewLifecycle.LIVE
    with pytest.raises(ValueError, match="ownership"):
        await runtime.renew("actor-b", "group:operators", request)


@pytest.mark.asyncio
async def test_closing_one_viewer_does_not_stop_the_shared_product() -> None:
    runtime = FixtureRuntime(_config())
    first = await runtime.open("actor-a", "group:operators", _open("browser-a"))
    await runtime.open("actor-b", "group:operators", _open("browser-b"))
    closed = await runtime.close(
        "actor-a",
        "group:operators",
        CloseLiveViewRequest(
            session_id=SESSION_ID,
            live_view_id=first.stream.live_view_id,
            viewer_instance_id="browser-a",
        ),
    )
    assert closed.closed
    state = await runtime.fixture_state()
    assert state.stream_products[0].active_viewers == 1
    assert state.stream_products[0].nvenc_sessions == 1
    assert state.stream_products[0].lifecycle.value == "ready"


def test_docs_index_lists_the_required_documents() -> None:
    assert [entry["id"] for entry in DOCS_INDEX] == ["agents", "design"]
    assert all(doc.body.strip() for doc in SERVER_DOCS)


def test_llms_txt_lists_every_document() -> None:
    assert LLMS_TXT.startswith("# anonymous-simulation\n")
    assert f"Contract revision {CONTRACT_REVISION}." in LLMS_TXT


def test_contract_defers_live_surface_to_discover() -> None:
    declaration = CONTRACT_DECLARATION.wire()
    assert declaration["server"] == "anonymous-simulation"
    assert declaration["contract_revision"] == 3
    assert "capabilities" not in declaration
    assert declaration["compliance"]


def test_parse_compliance_reads_status_and_note() -> None:
    items = parse_compliance(
        "# Manual\n\n## Contract Compliance\n\n"
        "- C01: met\n- C02: pending — reason text\n\n## Build And Test\n"
    )
    assert [(item.id, item.status, item.note) for item in items] == [
        ("C01", ComplianceStatus.MET, None),
        ("C02", ComplianceStatus.PENDING, "reason text"),
    ]


def test_stream_requires_token_protocol_and_view_identity() -> None:
    runtime = FixtureRuntime(_config())
    from anonymous_simulation_mcp.main import _authorized_stream

    messages: list[dict[str, object]] = []

    async def receive() -> dict[str, str]:
        return {"type": "websocket.connect"}

    async def send(message: dict[str, object]) -> None:
        messages.append(message)

    asyncio.run(
        _authorized_stream(
            runtime,
            {"headers": [], "query_string": b""},
            receive,
            send,
        )
    )
    assert messages == [{"type": "websocket.close", "code": 4401}]
