import base64
import json
from datetime import datetime, timezone
from types import SimpleNamespace

import mcp.types as types
import pytest
from pydantic import ValidationError

from datasheet_mcp import uris
from datasheet_mcp.catalog import ReportCursor, ReportEntry, ReportPage, UsageCursor
from datasheet_mcp.server import mcp_server
from veoveo_mcp.types import ResourceUri
from veoveo_mcp.contract.artifacts import ArtifactId
from veoveo_mcp.tasks import TaskError, TaskPage, TaskPageCursor, TaskStatus, new_task_id


def token(value):
    return base64.urlsafe_b64encode(json.dumps(value).encode()).decode().rstrip("=")


def test_cursor_and_resource_round_trips_bind_the_collection():
    task_id = new_task_id()
    now = datetime.now(timezone.utc)
    report = ReportCursor(task_id=task_id, created_at=now)
    usage = UsageCursor(task_id=task_id)
    for resource in (uris.ReportCatalogResource(report), uris.UsageCatalogResource(usage)):
        assert uris.parse_resource_uri(resource.to_uri()) == resource
    assert uris.parse_resource_uri(uris.usage_task_uri(task_id)).task_id == task_id
    assert uris.parse_resource_uri(uris.artifact_uri(ArtifactId(str(task_id)))).artifact_id == str(task_id)
    with pytest.raises(ValueError):
        uris.parse_resource_uri(ResourceUri(uris.REPORTS_URI + "?cursor=" + usage.encode()))
    with pytest.raises(ValueError):
        uris.parse_resource_uri(ResourceUri(uris.USAGE_ROOT_URI + "?cursor=" + report.encode()))
    with pytest.raises(TypeError):
        uris.usage_task_uri(str(task_id))


@pytest.mark.parametrize("suffix", [
    "?cursor=", "?cursor=x&cursor=y", "?unknown=x", "#fragment", "?cursor=%zz",
])
def test_catalog_rejects_unsupported_uri_components(suffix):
    with pytest.raises(ValueError):
        uris.parse_resource_uri(ResourceUri(uris.REPORTS_URI + suffix))


def test_cursor_rejects_extra_fields_versions_and_unbound_documents():
    value = {"version": 1, "collection": "usage", "task_id": str(new_task_id())}
    for invalid in [dict(value, version=True), dict(value, version=2),
                    dict(value, extra="x"), {key: item for key, item in value.items() if key != "collection"}]:
        with pytest.raises(ValueError):
            UsageCursor.decode(token(invalid))
    duplicate = base64.urlsafe_b64encode(b'{"version":1,"version":1,"collection":"usage"}').decode().rstrip("=")
    with pytest.raises(ValueError, match="duplicate"):
        UsageCursor.decode(duplicate)
    with pytest.raises(ValueError):
        UsageCursor.decode("a" * 769)
    for uri in ["datasheet://usage/task/not-a-task", "datasheet://usage/task/" + str(new_task_id()) + "/extra"]:
        try:
            assert uris.parse_resource_uri(ResourceUri(uri)) is None
        except TaskError:
            pass


def test_page_builder_rejects_wrong_continuation_and_oversize():
    now = datetime.now(timezone.utc)
    entry = ReportEntry(task_id=new_task_id(), task_type="profile_dataset", status=TaskStatus.QUEUED, created_at=now)
    with pytest.raises(ValidationError):
        ReportPage(items=(entry,) * 101)
    with pytest.raises(ValidationError):
        ReportPage(items=(entry,), next_cursor=ReportCursor(task_id=entry.task_id, created_at=now))
    with pytest.raises(ValidationError):
        ReportPage(items=(entry,) * 100, next_cursor=ReportCursor(task_id=new_task_id(), created_at=now))


def authenticated_server(tasks, monkeypatch):
    monkeypatch.setattr(mcp_server, "identity_from_scope", lambda _scope: object())
    monkeypatch.setattr(mcp_server, "request_scope", lambda _ctx: {})
    monkeypatch.setattr(mcp_server, "runtime_owner", lambda _identity: object())
    return mcp_server.build_mcp_server(SimpleNamespace(tasks=tasks))


async def test_discovery_never_reads_task_or_usage_state(monkeypatch):
    server = authenticated_server(object(), monkeypatch)
    result = await server.get_request_handler("resources/list").handler(None, None)
    assert uris.REPORTS_URI in {resource.uri for resource in result.resources}
    assert not any("/task/" in resource.uri for resource in result.resources)
    templates = await server.get_request_handler("resources/templates/list").handler(None, None)
    assert {uris.REPORTS_TEMPLATE, uris.USAGE_TEMPLATE} <= {value.uri_template for value in templates.resource_templates}


async def test_report_handler_follows_typed_cursor_and_emits_checked_pages(monkeypatch):
    now = datetime.now(timezone.utc)
    snapshots = tuple(SimpleNamespace(
        task_id=new_task_id(), created_at=now, status=TaskStatus.QUEUED, task_type="profile_dataset",
    ) for _ in range(103))
    calls = []

    class Query:
        def of_type(self, kind):
            assert kind.value == "profile_dataset"
            return self

        async def page(self, after, limit):
            calls.append((after, limit))
            if after is None:
                return TaskPage(snapshots[:100], TaskPageCursor(now, snapshots[99].task_id))
            assert after == TaskPageCursor(now, snapshots[99].task_id)
            return TaskPage(snapshots[100:], None)

    server = authenticated_server(SimpleNamespace(for_owner=lambda _owner: Query()), monkeypatch)
    read = server.get_request_handler("resources/read").handler
    context = SimpleNamespace(session=SimpleNamespace(client_capabilities=None), meta=None)
    first = json.loads((await read(context, types.ReadResourceRequestParams(uri=uris.REPORTS_URI))).contents[0].text)
    assert len(first["items"]) == 100
    assert first["next_cursor"]
    second = json.loads((await read(context, types.ReadResourceRequestParams(uri=first["next_uri"]))).contents[0].text)
    assert len(second["items"]) == 3
    assert second["next_cursor"] is None and second["next_uri"] is None
    assert len(calls) == 2
