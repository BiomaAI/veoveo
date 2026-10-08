from types import SimpleNamespace
from unittest.mock import AsyncMock, Mock

import pytest
from mcp.shared.exceptions import MCPError

from datasheet_mcp.server import task_extension
from veoveo_mcp.tasks import TaskNotFound, new_task_id


@pytest.mark.parametrize("operation", ["get_task", "cancel_task", "update_task"])
async def test_public_task_denial_uses_owner_query_and_never_trusted_runtime(operation, monkeypatch):
    task_id = new_task_id()
    query = SimpleNamespace(
        get=AsyncMock(return_value=None),
        cancel=AsyncMock(side_effect=TaskNotFound(str(task_id))),
        submit_input_responses=AsyncMock(side_effect=TaskNotFound(str(task_id))),
    )
    selection = Mock()
    selection.of_type.return_value = query
    tasks = SimpleNamespace(for_owner=Mock(return_value=selection))
    caller_owner = object()
    monkeypatch.setattr(task_extension, "runtime_owner", lambda _identity: caller_owner)
    extension = task_extension.DatasheetTaskExtension(SimpleNamespace(tasks=tasks))
    with pytest.raises(MCPError, match="unknown task id"):
        await getattr(extension, operation)(
            SimpleNamespace(identity=object()), None,
            SimpleNamespace(task_id=str(task_id), input_responses={}),
        )
    tasks.for_owner.assert_called_once_with(caller_owner)
    assert selection.of_type.call_args.args[0].value == "profile_dataset"
    method = {"get_task": query.get, "cancel_task": query.cancel,
              "update_task": query.submit_input_responses}[operation]
    assert method.await_args.args[0] == task_id


async def test_profile_submission_persists_current_arguments_and_worker_emits_current_product(monkeypatch):
    import asyncio
    import json
    from pathlib import Path
    from datasheet_mcp.contract import DatasetProfile, ProfileDatasetOutput, ProfileDatasetRequest
    from datasheet_mcp.server import profile_task
    from veoveo_mcp.contract import ArtifactMetadata, IssuedArtifactWriteCapability
    from veoveo_mcp.tasks import TaskStatus
    from veoveo_mcp.types import ChronoTimestamp

    task_id = new_task_id()
    capability = IssuedArtifactWriteCapability(
        capabilityId=str(new_task_id()), taskId=str(task_id), secret="fixture-only-capability-secret-00000000",
        expiresAt=ChronoTimestamp("2026-10-09T00:00:00Z"),
    )
    metadata_source = Path(__file__).resolve().parents[3] / "platform/artifacts/contract/tests/fixtures/metadata-output.json"
    metadata = ArtifactMetadata.model_validate_json(metadata_source.read_bytes()).presented_under_scheme("datasheet")
    drafts, products, updates = [], [], []

    async def create(draft):
        drafts.append(draft)
        return SimpleNamespace(snapshot=SimpleNamespace(task_id=draft.task_id, request=draft.request))

    async def scheduled(_state, snapshot):
        return snapshot

    async def put(_capability, _key, request, content):
        products.append((request, content))
        return metadata

    async def update(_state, _task_id, transition):
        updates.append(transition)

    state = SimpleNamespace(
        tasks=SimpleNamespace(create=create, store=SimpleNamespace(upsert_domain_usage=AsyncMock())),
        artifacts=SimpleNamespace(issue_write_capability=AsyncMock(return_value=capability), put_with_capability=put),
        max_dataset_bytes=1024, max_artifact_bytes=4096, logger=SimpleNamespace(warn=Mock()),
    )
    monkeypatch.setattr(profile_task, "runtime_owner", lambda _identity: object())
    monkeypatch.setattr(profile_task, "schedule_profile_task", scheduled)
    monkeypatch.setattr(profile_task, "update_task", update)
    request = ProfileDatasetRequest(inline_csv="name,value\na,1\nb,2\n", histogram_bins=2)
    snapshot = await profile_task.start_profile_task(state, object(), object(), request, frozenset())
    args = drafts[0].request["args"]
    assert args == {"datasetUri": None, "inlineCsv": request.inline_csv, "artifact": True, "histogramBins": 2}
    assert ProfileDatasetRequest.model_validate(args) == request
    # Native durable envelope names keep their existing profile; only owner args change.
    assert {"args", "dataset_b64", "dataset_name", "dataset_mime", "artifact_write_capability"} == snapshot.request.keys()
    await profile_task._run_task_inner(state, str(snapshot.task_id), snapshot.request, asyncio.Event())
    assert updates[-1].status() == TaskStatus.SUCCEEDED
    product_request, content = products[0]
    body = json.loads(content)
    assert body["rowCount"] == 2 and body["columnCount"] == 2
    assert DatasetProfile.model_validate_json(content).row_count == 2
    assert {"taskId", "artifactFormat", "rowCount", "columnCount"} == product_request.metadata.keys()
    assert product_request.metadata["artifactFormat"] == "datasheet_profile_json"
    output = ProfileDatasetOutput.model_validate(updates[-1].result().payload["structuredContent"])
    assert str(output.result_uri) == metadata.artifact_uri
    assert updates[-1].result_uri() == output.result_uri
    assert output.profile.row_count == 2
    assert state.tasks.store.upsert_domain_usage.await_args.kwargs["metadata"] == {"rowCount": 2, "columnCount": 2}
    for retired in [
        {"inline_csv": request.inline_csv, "artifact": True, "histogram_bins": 2},
        {**args, "inline_csv": request.inline_csv},
    ]:
        products.clear()
        await profile_task._run_task_inner(state, str(snapshot.task_id), {**snapshot.request, "args": retired}, asyncio.Event())
        assert not products
        assert updates[-1].result().payload["isError"] is True


@pytest.mark.parametrize("arguments", [
    {"inline_csv": "name,value\na,1\n"},
    {"inlineCsv": "name,value\na,1\n", "histogram_bins": 2},
    {"inlineCsv": "name,value\na,1\n", "inline_csv": "other"},
    {"inlineCsv": "name,value\na,1\n", "unknownField": 1},
])
async def test_task_argument_admission_refuses_retired_before_dispatch(arguments, monkeypatch):
    import mcp.types as types
    called = AsyncMock()
    monkeypatch.setattr(task_extension, "start_profile_task", called)
    extension = task_extension.DatasheetTaskExtension(SimpleNamespace())
    with pytest.raises(MCPError):
        await extension.start_tool_task(SimpleNamespace(identity=object(), plane=object()), None,
            types.CallToolRequestParams(name="profile_dataset", arguments=arguments))
    called.assert_not_awaited()
