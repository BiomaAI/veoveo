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
                        request["run_id"],
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


if __name__ == "__main__":
    unittest.main()
