#!/usr/bin/env bash
# Run the headless Rerun viewer and the stdio bridge as one failure unit:
# `rerun viewer-mcp` can only dial a viewer on localhost, so both processes
# must share the container. If either exits, the container exits.
set -euo pipefail

rerun --headless --bind 0.0.0.0 &
bridge "$@" &
wait -n
exit 1
