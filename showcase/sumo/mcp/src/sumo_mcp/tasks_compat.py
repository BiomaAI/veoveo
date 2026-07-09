"""The task-serving seam.

All use of the MCP SDK's task API is funneled through this one module so the
eventual migration to the SEP-2663 tasks *extension* (wire-incompatible with the
SEP-1686 lifecycle the workspace's gateway/kernel speak today) is a localized
change. Today it wraps `mcp==1.28.x`'s experimental task API, which implements
exactly the 2025-11-25 SEP-1686 shape our Rust stack projects.

Keep the deprecation warning filtered here, not scattered across the server.
"""

from __future__ import annotations

import warnings
from collections.abc import Awaitable, Callable

warnings.filterwarnings(
    "ignore",
    message="The experimental tasks API is deprecated",
)

from mcp import types  # noqa: E402
from mcp.server.experimental.task_context import ServerTaskContext  # noqa: E402
from mcp.server.lowlevel import Server  # noqa: E402

# Task execution modes, re-exported so the server never imports SDK task symbols
# directly.
TASK_REQUIRED = types.TASK_REQUIRED
TASK_OPTIONAL = types.TASK_OPTIONAL
TASK_FORBIDDEN = types.TASK_FORBIDDEN

TaskWork = Callable[[ServerTaskContext], Awaitable[types.CallToolResult]]


def enable_tasks(server: Server) -> None:
    """Turn on task support and auto-register tasks/get|result|list|cancel."""
    server.experimental.enable_tasks()


def tool_execution(mode: str) -> types.ToolExecution:
    """Build the `execution` field advertising a tool's task support."""
    return types.ToolExecution(taskSupport=mode)


async def run_as_task(server: Server, work: TaskWork) -> types.CreateTaskResult:
    """Start background work and return CreateTaskResult immediately.

    The tool handler validates the request is task-augmented, then hands the
    long op to `run_task`; the SDK completes the task when `work` returns and
    fails it if `work` raises. `tasks/result` blocks until terminal per spec.
    """
    ctx = server.request_context
    ctx.experimental.validate_task_mode(TASK_REQUIRED)
    return await ctx.experimental.run_task(work)


def is_task_request(server: Server) -> bool:
    return server.request_context.experimental.is_task
