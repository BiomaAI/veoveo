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
