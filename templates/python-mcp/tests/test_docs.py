"""The datasheet well-known surface: documents, declaration, admin projection."""

import asyncio
import json
from typing import Any

from veoveo_mcp.contract import (
    CHECKLIST_IDS,
    CONTRACT_REVISION,
    ComplianceStatus,
    REQUIRED_AGENT_SECTIONS,
)

from datasheet_mcp import uris
from datasheet_mcp.docs import (
    CAPABILITY_INVENTORY,
    CONTRACT_DECLARATION,
    LLMS_TXT,
    SERVER_DOCS,
)
from datasheet_mcp.server.main import RootApp


def test_docs_index_lists_the_embedded_documents():
    index = json.loads(json.dumps(SERVER_DOCS.index_wire()))
    assert index == [
        {"id": "agents", "title": "Agent work manual"},
        {"id": "design", "title": "Domain design"},
    ]


def test_llms_txt_renders_the_contract_format():
    assert LLMS_TXT == (
        "# datasheet\n\n"
        "> Veoveo MCP server documents. Contract revision 2.\n\n"
        "## Docs\n\n"
        "- [Agent work manual](agents)\n"
        "- [Domain design](design)\n"
    )


def test_agent_manual_contains_the_required_sections():
    manual = SERVER_DOCS.agent_manual()
    assert manual is not None
    for section in REQUIRED_AGENT_SECTIONS:
        assert section in manual


def test_declaration_states_revision_2_with_a_dense_checklist():
    assert CONTRACT_DECLARATION.server == "datasheet"
    assert CONTRACT_DECLARATION.contract_revision == CONTRACT_REVISION == 2
    declared = [item.id for item in CONTRACT_DECLARATION.compliance]
    assert declared == list(CHECKLIST_IDS)


def test_declaration_meets_the_well_known_surface_items():
    by_id = {item.id: item for item in CONTRACT_DECLARATION.compliance}
    for item_id in ("C18", "C19", "C20", "C21"):
        assert by_id[item_id].status == ComplianceStatus.MET
    for item in CONTRACT_DECLARATION.compliance:
        if item.status == ComplianceStatus.PENDING:
            assert item.note, f"{item.id} is pending without a reason"


def test_declaration_wire_shape_and_capability_inventory():
    wire = CONTRACT_DECLARATION.wire()
    assert set(wire) == {"server", "contract_revision", "compliance", "capabilities"}
    assert wire["server"] == "datasheet"
    assert wire["contract_revision"] == 2
    assert wire["capabilities"]["tools"] == list(CAPABILITY_INVENTORY.tools)
    assert uris.DOCS_URI in wire["capabilities"]["resources"]
    assert uris.CONTRACT_URI in wire["capabilities"]["resources"]
    assert wire["capabilities"]["tasks"] == ["profile_dataset"]
    assert json.loads(json.dumps(wire)) == wire


def _root_app() -> RootApp:
    async def protected_app(
        scope: dict[str, Any], receive: Any, send: Any
    ) -> None:
        path = scope["path"]
        if path == "/datasheet/admin/docs/llms.txt":
            body = LLMS_TXT.encode()
            await send(
                {
                    "type": "http.response.start",
                    "status": 200,
                    "headers": [],
                }
            )
            await send({"type": "http.response.body", "body": body})
            return
        doc = SERVER_DOCS.doc(path.removeprefix("/datasheet/admin/docs/"))
        if doc is None:
            body = b"unknown server document"
            status = 404
            content_type = b"text/plain; charset=utf-8"
        else:
            body = doc.body.encode()
            status = 200
            content_type = b"text/markdown; charset=utf-8"
        await send(
            {
                "type": "http.response.start",
                "status": status,
                "headers": [(b"content-type", content_type)],
            }
        )
        await send({"type": "http.response.body", "body": body})

    return RootApp(
        health_path="/datasheet/healthz",
        ready_path="/datasheet/readyz",
        docs_llms_path="/datasheet/admin/docs/llms.txt",
        docs_prefix="/datasheet/admin/docs/",
        mcp_path="/datasheet/mcp",
        protected_app=protected_app,
        # The session manager is only touched by the lifespan protocol.
        session_manager=None,  # type: ignore[arg-type]
        ready=asyncio.Event(),
    )


def _get(app: RootApp, path: str) -> tuple[int, dict[bytes, bytes], bytes]:
    messages: list[dict[str, Any]] = []

    async def send(message: dict[str, Any]) -> None:
        messages.append(message)

    async def receive() -> dict[str, Any]:
        return {"type": "http.request", "body": b"", "more_body": False}

    scope = {"type": "http", "method": "GET", "path": path}
    asyncio.run(app(scope, receive, send))
    start = messages[0]
    body = b"".join(m.get("body", b"") for m in messages[1:])
    return start["status"], dict(start["headers"]), body


def test_admin_projection_routes_llms_txt_through_the_protected_stack():
    status, _headers, body = _get(_root_app(), "/datasheet/admin/docs/llms.txt")
    assert status == 200
    assert body.decode() == LLMS_TXT


def test_admin_projection_serves_document_bodies():
    app = _root_app()
    for doc in SERVER_DOCS:
        status, headers, body = _get(app, f"/datasheet/admin/docs/{doc.id}")
        assert status == 200
        assert headers[b"content-type"] == b"text/markdown; charset=utf-8"
        assert body.decode() == doc.body


def test_admin_projection_rejects_unknown_documents():
    status, _headers, _body = _get(_root_app(), "/datasheet/admin/docs/missing")
    assert status == 404
