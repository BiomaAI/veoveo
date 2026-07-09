"""Live MCP client for end-to-end verification: exercises the served sumo-mcp
endpoint over streamable HTTP against the real running container (real TraCI world).

Proves the full surface end to end: initialize, list tools with task modes, a
sync read (query_state over the live SUMO world), and the task path
(call_tool_as_task -> poll -> get_task_result) for run_batch — the exact
detach/sleep/wake the agent kernel drives. Exits non-zero on any failure.
"""

from __future__ import annotations

import os
import sys

import anyio
from mcp import types
from mcp.client.experimental.task_handlers import ExperimentalTaskHandlers
from mcp.client.session import ClientSession
from mcp.client.streamable_http import streamablehttp_client

URL = os.environ.get("SUMO_MCP_URL", "http://127.0.0.1:8795/mcp")


async def main() -> int:
    async with streamablehttp_client(URL) as (read, write, _get_sid):
        async with ClientSession(
            read, write, experimental_task_handlers=ExperimentalTaskHandlers()
        ) as client:
            await client.initialize()

            listed = await client.list_tools()
            by_name = {t.name: t for t in listed.tools}
            for required in ("query_state", "describe_scenario", "run_batch", "set_signal_phase"):
                assert required in by_name, f"missing tool {required}"
            assert by_name["query_state"].execution.taskSupport == types.TASK_FORBIDDEN
            assert by_name["run_batch"].execution.taskSupport == types.TASK_REQUIRED
            print(f"OK  list_tools: {len(by_name)} tools, task modes correct")

            # Sync read against the live SUMO world.
            state = (await client.call_tool("query_state", {})).structuredContent
            print(f"OK  query_state: {state['vehicle_count']} vehicles, "
                  f"mean_speed={state['mean_speed_mps']:.2f} m/s @ t={state['sim_time_s']}")

            # Describe the scenario the SUMO container loaded.
            desc = (await client.call_tool("describe_scenario", {})).structuredContent
            print(f"OK  describe_scenario: {desc['name']} · "
                  f"{desc['edge_count']} edges · {desc['signal_count']} signals "
                  f"@ ({desc['origin_lat']:.4f}, {desc['origin_lon']:.4f})")

            # Task path: run_batch detaches, we poll to terminal, read the result.
            created = await client.experimental.call_tool_as_task(
                "run_batch", {"steps": 50}, ttl=120_000
            )
            task_id = created.task.taskId
            print(f"OK  run_batch detached as task {task_id} (status={created.task.status})")
            snapshot = created.task
            async for snapshot in client.experimental.poll_task(task_id):
                if snapshot.status in ("completed", "failed", "cancelled"):
                    break
            assert snapshot.status == "completed", f"task ended {snapshot.status}"
            payload = await client.experimental.get_task_result(task_id, types.CallToolResult)
            assert not payload.isError
            body = payload.structuredContent
            assert body["steps_advanced"] == 50
            print(f"OK  tasks/result: advanced {body['steps_advanced']} steps, "
                  f"congestion_detected={body['congestion_detected']}")

            # Actuate the real network: signal phase, speed limit, lane close/open.
            if desc["signals"]:
                sig = desc["signals"][0]
                ack = (await client.call_tool(
                    "set_signal_phase", {"signal_id": sig, "phase": 0}
                )).structuredContent
                print(f"OK  set_signal_phase({sig}) -> {ack['detail']}")

            edge = desc["edges"][0]
            ack = (await client.call_tool(
                "set_edge_speed", {"edge_id": edge, "speed_mps": 8.0}
            )).structuredContent
            print(f"OK  set_edge_speed -> {ack['detail']}")

            lane = f"{edge}_0"
            closed = (await client.call_tool("close_lane", {"lane_id": lane})).structuredContent
            reopened = (await client.call_tool("open_lane", {"lane_id": lane})).structuredContent
            print(f"OK  incident: {closed['detail']} then {reopened['detail']}")

    print("VERIFY OK — live SUMO world driven end to end over MCP")
    return 0


if __name__ == "__main__":
    sys.exit(anyio.run(main))
