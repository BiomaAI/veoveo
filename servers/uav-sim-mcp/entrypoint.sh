#!/bin/sh
set -u

/usr/local/bin/uav-sim-mcp &
child=$!
trap 'kill -TERM "$child" 2>/dev/null || true' TERM INT
wait "$child"
exit $?
