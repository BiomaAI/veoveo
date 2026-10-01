"""Real stateless HTTP negotiation, conditional reads and caller denial."""
import asyncio
from types import SimpleNamespace

import httpx
import mcp.types as types
import pytest
from mcp.server.streamable_http_manager import StreamableHTTPSessionManager
from mcp.shared.exceptions import MCPError
from mcp.shared.inbound import encode_header_value

from datasheet_mcp.server import mcp_server
from veoveo_mcp.contract.knowledge import EXTENSION_ID, OBSERVATION_KEY, Observation


@pytest.mark.asyncio
async def test_docs_use_request_capabilities_and_reauthorize_conditions(monkeypatch):
    admissions = []

    def identity(scope):
        authorization = dict(scope["headers"]).get(b"authorization")
        admissions.append(authorization)
        if authorization != b"Bearer fixture-read":
            raise MCPError(code=types.INVALID_REQUEST, message="fixture caller denied")
        return object()

    monkeypatch.setattr(mcp_server, "identity_from_scope", identity)
    server = mcp_server.build_mcp_server(SimpleNamespace(tasks=object()))
    manager = StreamableHTTPSessionManager(server, json_response=True, stateless=True)
    async with asyncio.timeout(10), manager.run():
        async with httpx.AsyncClient(
            transport=httpx.ASGITransport(app=manager.handle_request),
            base_url="http://localhost",
            headers={"Accept": "application/json, text/event-stream",
                     "MCP-Protocol-Version": "2026-07-28"},
        ) as client:
            async def request(method, params, token="fixture-read", expected_status=200):
                metadata = params.setdefault("_meta", {})
                metadata.setdefault("io.modelcontextprotocol/protocolVersion", "2026-07-28")
                metadata.setdefault("io.modelcontextprotocol/clientCapabilities", {})
                headers = {"Authorization": f"Bearer {token}", "Mcp-Method": method}
                if method == "resources/read":
                    headers["Mcp-Name"] = encode_header_value(params["uri"])
                response = await client.post("/datasheet/mcp", headers=headers,
                    json={"jsonrpc": "2.0", "id": 1, "method": method, "params": params})
                assert response.status_code == expected_status, response.text
                return response.json()

            discovery = await request("server/discover", {})
            assert discovery["result"]["capabilities"]["extensions"][EXTENSION_ID] == {}
            uri = "datasheet://docs/design"
            meta = {"io.modelcontextprotocol/clientCapabilities": {"extensions": {EXTENSION_ID: {}}}}
            full = await request("resources/read", {"uri": uri, "_meta": meta})
            observation = Observation.model_validate(full["result"]["_meta"][OBSERVATION_KEY])
            meta[EXTENSION_ID] = {"ifNoneMatch": str(observation.revision)}
            conditional = await request("resources/read", {"uri": uri, "_meta": meta})
            assert conditional["result"]["contents"] == []
            assert conditional["result"]["_meta"][OBSERVATION_KEY]["notModified"]
            denied = await request("resources/read", {"uri": uri, "_meta": meta}, token="fixture-denied", expected_status=400)
            assert denied["error"]["code"] == types.INVALID_REQUEST
            ordinary = await request("resources/read", {"uri": uri})
            assert ordinary["result"]["contents"]
            assert OBSERVATION_KEY not in ordinary["result"].get("_meta", {})
            assert admissions == [b"Bearer fixture-read", b"Bearer fixture-read",
                                  b"Bearer fixture-denied", b"Bearer fixture-read"]
