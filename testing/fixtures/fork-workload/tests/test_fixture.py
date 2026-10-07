from __future__ import annotations

import asyncio

import pytest

from veoveo_mcp.contract import CONTRACT_REVISION, requirement_catalog

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
        sessionId=SESSION_ID,
        cameraId=CAMERA_ID,
        viewerInstanceId=instance,
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
        sessionId=SESSION_ID,
        liveViewId=first.stream.live_view_id,
        viewerInstanceId="browser-a",
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
            sessionId=SESSION_ID,
            liveViewId=first.stream.live_view_id,
            viewerInstanceId="browser-a",
        ),
    )
    assert closed.closed
    state = await runtime.fixture_state()
    assert state.stream_products[0].active_viewers == 1
    assert state.stream_products[0].nvenc_sessions == 1
    assert state.stream_products[0].lifecycle.value == "ready"


def test_docs_index_lists_the_required_documents() -> None:
    assert [entry["id"] for entry in DOCS_INDEX["items"]] == ["agents", "design"]
    assert all(doc.body.strip() for doc in SERVER_DOCS)


def test_llms_txt_lists_every_document() -> None:
    assert LLMS_TXT.startswith("# anonymous-simulation\n")
    assert f"Contract revision {CONTRACT_REVISION}." in LLMS_TXT


def test_contract_defers_live_surface_to_discover() -> None:
    declaration = CONTRACT_DECLARATION.wire()
    assert declaration["server"] == "anonymous-simulation"
    assert declaration["contractRevision"] == CONTRACT_REVISION
    assert "capabilities" not in declaration
    assert declaration["compliance"]


def test_profile_is_complete_and_not_derived_from_markdown() -> None:
    from anonymous_simulation_mcp.mcp_server import SERVER_DOCS, CONTRACT_DECLARATION
    assert SERVER_DOCS.profile is not None
    assert CONTRACT_DECLARATION.wire() == SERVER_DOCS.profile.wire()
    assert tuple(item.id for item in SERVER_DOCS.profile.compliance) == requirement_catalog().ids


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


@pytest.mark.asyncio
async def test_current_wire_family_refuses_retired_and_mixed_keys() -> None:
    import json
    from pydantic import TypeAdapter, ValidationError
    from anonymous_simulation_mcp.contract import ListLiveCamerasRequest

    runtime = FixtureRuntime(_config())
    opened = await runtime.open("actor-a", "group:operators", _open("browser-a"))
    state = await runtime.fixture_state()
    models = [
        _open("browser-a"),
        ListLiveCamerasRequest(sessionId=SESSION_ID),
        RenewLiveViewRequest(sessionId=SESSION_ID, liveViewId=opened.stream.live_view_id, viewerInstanceId="browser-a"),
        CloseLiveViewRequest(sessionId=SESSION_ID, liveViewId=opened.stream.live_view_id, viewerInstanceId="browser-a"),
        opened, opened.stream, opened.stream.source_region, opened.stream.endpoint,
        state, state.cameras[0], state.stream_products[0], state.stream_products[0].camera_regions[0],
    ]
    for model in models:
        cls = type(model)
        adapter = TypeAdapter(cls)
        python = model.model_dump(mode="python", by_alias=True)
        wire = model.model_dump(mode="json", by_alias=True)
        assert cls.model_validate(python) == model
        assert cls.model_validate_json(json.dumps(wire)) == model
        assert adapter.validate_python(python) == model
        assert adapter.validate_json(json.dumps(wire)) == model
        for field_name, field in cls.model_fields.items():
            alias = field.alias or field_name
            if alias == field_name:
                continue
            for mixed in [False, True]:
                bad = dict(wire)
                value = bad[alias] if mixed else bad.pop(alias)
                bad[field_name] = value
                for decode, value in [(cls.model_validate, bad), (cls.model_validate_json, json.dumps(bad)), (adapter.validate_python, bad), (adapter.validate_json, json.dumps(bad))]:
                    with pytest.raises(ValidationError):
                        decode(value)
        if "schemaVersion" in wire:
            absent = dict(wire); absent.pop("schemaVersion")
            for decode, value in [(cls.model_validate, absent), (cls.model_validate_json, json.dumps(absent)), (adapter.validate_python, absent), (adapter.validate_json, json.dumps(absent))]:
                with pytest.raises(ValidationError):
                    decode(value)
        if "schemaVersion" in wire:
            bad = dict(wire, schemaVersion="veoveo.ai/retired/v1")
            for decode, value in [(cls.model_validate, bad), (cls.model_validate_json, json.dumps(bad)), (adapter.validate_python, bad), (adapter.validate_json, json.dumps(bad))]:
                with pytest.raises(ValidationError):
                    decode(value)
    # Nested actual producer values enter through the complete outer receiver.
    for path in [("stream", "sourceRegion", "cameraId"), ("stream", "endpoint", "streamUrl")]:
        for mixed in [False, True]:
            bad = opened.model_dump(mode="json", by_alias=True)
            obj = bad[path[0]][path[1]]
            old = "camera_id" if path[2] == "cameraId" else "stream_url"
            value = obj[path[2]] if mixed else obj.pop(path[2])
            obj[old] = value
            for decode, value in [(type(opened).model_validate, bad), (type(opened).model_validate_json, json.dumps(bad)), (TypeAdapter(type(opened)).validate_python, bad), (TypeAdapter(type(opened)).validate_json, json.dumps(bad))]:
                with pytest.raises(ValidationError):
                    decode(value)
    await runtime.close("actor-a", "group:operators", CloseLiveViewRequest(sessionId=SESSION_ID, liveViewId=opened.stream.live_view_id, viewerInstanceId="browser-a"))


@pytest.mark.asyncio
async def test_actual_mcp_refuses_retired_requests_before_runtime_effects() -> None:
    from datetime import datetime, timedelta, timezone
    from starlette.requests import Request
    import mcp.types as types
    from mcp.server import ServerRequestContext
    from veoveo_mcp.contract.identity import GatewayInternalIdentity
    from veoveo_mcp.internal_auth import IDENTITY_SCOPE_KEY
    from anonymous_simulation_mcp.mcp_server import build_mcp_server

    runtime = FixtureRuntime(_config())
    now = datetime.now(timezone.utc)
    identity = GatewayInternalIdentity.model_validate({
        "issuer": "acceptance", "profile": "operator", "server": "anonymous-simulation",
        "actor": {"id": "acceptance-actor", "kind": "service", "issuer": "acceptance", "subject": "acceptance", "tenant": "acceptance", "scopes": ["operator:use"]},
        "authority": {"work_context": "acceptance", "tenant": "acceptance", "membership": "contributor", "policy_revision": "acceptance", "output_policy": {"owner": {"kind": "group", "id": "operators"}}, "provenance": {"mode": "automated"}},
        "jwt_id": "acceptance-request", "issued_at": now, "not_before": now, "expires_at": now + timedelta(minutes=1),
    })
    server = build_mcp_server(runtime)
    handler = server.get_request_handler("tools/call")
    assert handler is not None
    request = Request({"type": "http", "method": "POST", "path": "/anonymous-simulation/mcp", "headers": [], IDENTITY_SCOPE_KEY: identity})
    ctx = ServerRequestContext(session=None, lifespan_context={}, protocol_version="2026-07-28", method="tools/call", request=request)
    current = _open("browser-a").model_dump(mode="json", by_alias=True)
    before = await runtime.fixture_state()
    for old, canonical in [("session_id", "sessionId"), ("camera_id", "cameraId"), ("viewer_instance_id", "viewerInstanceId")]:
        for mixed in [False, True]:
            bad = dict(current)
            value = bad[canonical] if mixed else bad.pop(canonical)
            bad[old] = value
            result = await handler.handler(ctx, types.CallToolRequestParams(name="open_live_view", arguments=bad))
            assert result.is_error
            assert await runtime.fixture_state() == before
    result = await handler.handler(ctx, types.CallToolRequestParams(name="open_live_view", arguments=current))
    assert not result.is_error
    opened = result.structured_content
    assert isinstance(opened, dict)
    view_id = opened["stream"]["liveViewId"]
    after = await runtime.fixture_state()
    for tool in ["renew_live_view", "close_live_view"]:
        current_control = {"sessionId": SESSION_ID, "liveViewId": view_id, "viewerInstanceId": "browser-a"}
        for old, canonical in [("session_id", "sessionId"), ("live_view_id", "liveViewId"), ("viewer_instance_id", "viewerInstanceId")]:
            for mixed in [False, True]:
                bad = dict(current_control)
                value = bad[canonical] if mixed else bad.pop(canonical)
                bad[old] = value
                result = await handler.handler(ctx, types.CallToolRequestParams(name=tool, arguments=bad))
                assert result.is_error
                assert await runtime.fixture_state() == after
    await runtime.close("acceptance-actor", "group:operators", CloseLiveViewRequest(sessionId=SESSION_ID, liveViewId=view_id, viewerInstanceId="browser-a"))
