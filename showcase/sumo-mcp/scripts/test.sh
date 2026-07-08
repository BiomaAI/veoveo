#!/usr/bin/env bash
# Provision an isolated env for the SUMO MCP server and run its test suite.
# Uses the fake sim driver, so no SUMO install is needed.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VENV="${SUMO_MCP_VENV:-/tmp/veoveo-sumo-venv}"

if [ ! -x "$VENV/bin/python3" ]; then
  uv venv "$VENV" --python 3.11 >&2
fi
uv pip install --python "$VENV/bin/python3" -e "$HERE[dev]" >&2

"$VENV/bin/python3" -m pytest "$HERE" -q
