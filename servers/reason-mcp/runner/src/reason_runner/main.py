"""Runner entrypoint. Reads the typed request, writes the typed response."""

from __future__ import annotations

import argparse
import logging
import os
import sys
from pathlib import Path


def main() -> None:
    # The server contract requires an empty stdout: the typed response file is
    # the only output channel. Point fd 1 at stderr before any library — the
    # inference runtime included — can print to it.
    os.dup2(2, 1)
    logging.basicConfig(stream=sys.stderr, level=logging.INFO)
    parser = argparse.ArgumentParser(prog="reason-runner")
    parser.add_argument("--request-json", required=True, type=Path)
    parser.add_argument("--response-json", required=True, type=Path)
    arguments = parser.parse_args()

    from . import inference, protocol

    request = protocol.parse_request(arguments.request_json.read_bytes())
    if Path(request.response_json) != arguments.response_json:
        raise SystemExit("request response_json does not match --response-json")
    response = inference.run(request)
    payload = response.to_json().encode("utf-8")
    if len(payload) > request.max_response_bytes:
        raise SystemExit(
            f"response is {len(payload)} bytes and exceeds "
            f"max_response_bytes ({request.max_response_bytes})"
        )
    staging = arguments.response_json.with_suffix(".tmp")
    staging.write_bytes(payload)
    os.replace(staging, arguments.response_json)


if __name__ == "__main__":
    main()
