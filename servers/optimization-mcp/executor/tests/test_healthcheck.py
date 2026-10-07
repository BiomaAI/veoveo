import asyncio
import json
import tempfile
import unittest
from pathlib import Path

from veoveo_cuopt_executor import healthcheck
from veoveo_cuopt_executor.protocol import read_frame, response, write_frame


class HealthcheckTests(unittest.IsolatedAsyncioTestCase):
    async def test_requires_a_ready_typed_health_response(self) -> None:
        fixture = json.loads((Path(__file__).parents[2] / "testdata/executor-protocol.json").read_text())
        health_result = fixture["responses"][0]["result"]
        with tempfile.TemporaryDirectory() as directory:
            socket_path = Path(directory) / "executor.sock"

            async def serve(
                reader: asyncio.StreamReader,
                writer: asyncio.StreamWriter,
            ) -> None:
                request = await read_frame(reader, 4096)
                await write_frame(
                    writer,
                    response(
                        request['runId'],
                        health_result,
                    ),
                    4096,
                )
                writer.close()
                await writer.wait_closed()

            server = await asyncio.start_unix_server(serve, path=socket_path)
            previous = healthcheck.SOCKET_PATH
            healthcheck.SOCKET_PATH = socket_path
            try:
                await healthcheck.check()
            finally:
                healthcheck.SOCKET_PATH = previous
                server.close()
                await server.wait_closed()

    async def test_actual_health_receiver_refuses_retired_mixed_and_foreign_parent(self) -> None:
        import copy
        fixture = json.loads((Path(__file__).parents[2] / "testdata/executor-protocol.json").read_text())
        current = response("run-healthcheck", fixture["responses"][0]["result"])
        mutations = []
        for path, key, retired in [((), "runId", "run_id"), (("result", "health"), "gpuUuid", "gpu_uuid")]:
            for mixed in [False, True]:
                changed = copy.deepcopy(current)
                target = changed
                for component in path:
                    target = target[component]
                target[retired] = target[key] if mixed else target.pop(key)
                mutations.append(changed)
        mutations.append({**current, "runId": "run-foreign"})
        mutations.append({**current, "protocol": "veoveo.ai/cuopt-executor/v1"})
        for changed in mutations:
            with tempfile.TemporaryDirectory() as directory:
                socket_path = Path(directory) / "executor.sock"
                async def serve(reader, writer):
                    await read_frame(reader, 4096)
                    # Raw current-format peer corruption must reach the probe receiver.
                    body = json.dumps(changed).encode()
                    writer.write(len(body).to_bytes(8, "big") + body)
                    await writer.drain()
                    writer.close()
                    await writer.wait_closed()
                server = await asyncio.start_unix_server(serve, path=socket_path)
                previous = healthcheck.SOCKET_PATH
                healthcheck.SOCKET_PATH = socket_path
                try:
                    with self.assertRaises(RuntimeError):
                        await healthcheck.check()
                finally:
                    healthcheck.SOCKET_PATH = previous
                    server.close()
                    await server.wait_closed()


if __name__ == "__main__":
    unittest.main()
