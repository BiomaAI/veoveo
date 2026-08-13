#!/usr/bin/env bash
# Run the headless Rerun viewer and its explicit legacy-protocol adapter as one
# failure unit. `rerun viewer-mcp` can only dial a viewer on localhost.
set -euo pipefail

rerun --headless --bind 0.0.0.0 &
legacy-bridge "$@" &
wait -n
exit 1
