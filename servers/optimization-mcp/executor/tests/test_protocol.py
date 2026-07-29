import asyncio
import json
import unittest

from veoveo_cuopt_executor.protocol import (
    PROTOCOL_VERSION,
    ProtocolError,
    error_response,
    read_frame,
)


class ProtocolTests(unittest.IsolatedAsyncioTestCase):
    async def test_reads_a_bounded_big_endian_frame(self) -> None:
        request = {
            "protocol": PROTOCOL_VERSION,
            "run_id": "run-018f0000-0000-7000-8000-000000000000",
            "operation": {"operation": "health"},
        }
        body = json.dumps(request).encode()
        reader = asyncio.StreamReader()
        reader.feed_data(len(body).to_bytes(8, "big") + body)
        reader.feed_eof()
        self.assertEqual(await read_frame(reader, 4096), request)

    async def test_rejects_an_oversized_frame_before_body_read(self) -> None:
        reader = asyncio.StreamReader()
        reader.feed_data((4097).to_bytes(8, "big"))
        reader.feed_eof()
        with self.assertRaises(ProtocolError):
            await read_frame(reader, 4096)

    def test_errors_are_typed_protocol_results(self) -> None:
        result = error_response(
            "run-018f0000-0000-7000-8000-000000000000",
            "invalid_request",
            "bad request",
        )
        self.assertEqual(result["result"]["result"], "error")
        self.assertEqual(
            result["result"]["error"]["code"], "invalid_request"
        )


if __name__ == "__main__":
    unittest.main()
