#!/usr/bin/env bash
# Create/refresh the Python env the hub-catalog cross-check needs: the Rerun
# SDK's redap catalog client plus its DataFusion query engine, version-pinned
# to the same 0.34 line as the workspace's Rerun crates.
#
# Prints the interpreter path on stdout; set HUB_PYTHON to it for the smoke.
set -euo pipefail

VENV="${HUB_VENV:-/tmp/veoveo-hub-venv}"

if [ ! -x "$VENV/bin/python3" ]; then
  uv venv "$VENV" --python 3.11 >&2
fi
# rerun 0.34's catalog client requires datafusion major version 53.
uv pip install --python "$VENV/bin/python3" \
  "rerun-sdk==0.34.0" "datafusion>=53,<54" "pyarrow" >&2

echo "$VENV/bin/python3"
