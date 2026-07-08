#!/usr/bin/env bash
# Live capstone: bring up the whole SUMO showcase (SUMO + sumo-mcp + hub),
# prove SUMO's world is captured durably in the hub, then drive the served MCP
# endpoint end to end (sync read + task detach/poll/result + actuation).
#
# This is the one path the unit/smoke tiers can't cover: a real SUMO container
# stepping a real network, a real TraCI connection, a real push into the Rust
# hub, and a real MCP client over streamable HTTP.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
COMPOSE=(docker compose -f compose.yaml -f showcase/compose.showcase.yaml --profile hub --profile showcase)
PYBIN="${CAPSTONE_PYTHON:-/tmp/veoveo-sumo-venv/bin/python}"

cleanup() { echo "==> tearing down"; "${COMPOSE[@]}" down -v >/dev/null 2>&1 || true; }
trap cleanup EXIT

echo "==> bringing up hub + showcase (images already built)"
"${COMPOSE[@]}" up -d

echo "==> waiting for containers to be running"
for svc in hub-spooler sumo sumo-mcp; do
  for _ in $(seq 1 60); do
    state=$("${COMPOSE[@]}" ps -a --format '{{.Service}} {{.State}}' | awk -v s="$svc" '$1==s{print $2}')
    [ "$state" = "running" ] && { echo "    $svc: running"; break; }
    sleep 1
  done
  [ "$state" = "running" ] || { echo "FAIL: $svc did not reach running ($state)"; "${COMPOSE[@]}" logs "$svc" | tail -30; exit 1; }
done

echo "==> waiting for sumo-mcp HTTP endpoint (127.0.0.1:8795)"
for _ in $(seq 1 60); do
  # streamable-HTTP: a bare GET without a session returns 400/406, which still
  # proves the listener is up.
  code=$(curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:8795/mcp || true)
  [ "$code" != "000" ] && { echo "    endpoint answering (HTTP $code)"; break; }
  sleep 1
done
[ "$code" != "000" ] || { echo "FAIL: sumo-mcp endpoint never answered"; "${COMPOSE[@]}" logs sumo-mcp | tail -40; exit 1; }

echo "==> letting SUMO step and sumo-mcp push into the hub (~20s)"
sleep 20

echo "==> proving the SUMO world is durable in the hub (QueryEngine)"
Q=$("${COMPOSE[@]}" exec -T hub-spooler hub-query \
      --root /var/lib/veoveo/spool/world \
      --entities '/world/sumo/**' --timeline tick 2>/dev/null || true)
echo "    $Q"
echo "$Q" | grep -q '"sumo-live"' || { echo "FAIL: no sumo-live recording captured in hub"; "${COMPOSE[@]}" logs sumo-mcp | tail -40; exit 1; }
ROWS=$(echo "$Q" | "$PYBIN" -c 'import sys,json;print(json.load(sys.stdin)["rows_by_recording"].get("sumo-live",0))')
[ "$ROWS" -gt 0 ] || { echo "FAIL: sumo-live captured 0 rows"; exit 1; }
echo "OK  hub captured $ROWS rows of the live SUMO world under /world/sumo/**"

echo "==> driving the served MCP endpoint end to end"
SUMO_MCP_URL="http://127.0.0.1:8795/mcp" "$PYBIN" showcase/scripts/capstone_client.py

echo "showcase capstone ok"
