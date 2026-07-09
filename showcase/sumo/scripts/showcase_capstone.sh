#!/usr/bin/env bash
# Live capstone: bring up the whole SUMO showcase (SUMO + sumo-mcp + hub),
# prove SUMO's world is captured durably in the hub, then drive the served MCP
# endpoint end to end (sync read + task detach/poll/result + actuation).
#
# This is the one path the unit/smoke tiers can't cover: a real SUMO container
# stepping a real network, a real TraCI connection, a real push into the Rust
# hub, and a real MCP client over streamable HTTP.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT"
COMPOSE=(docker compose -f compose.yaml -f showcase/sumo/compose.showcase.yaml --profile hub --profile showcase)
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

# sumo-mcp connects TraCI (waiting for LuST to load) before it serves HTTP, so a
# live endpoint already means SUMO is up and the push loop is running.
echo "==> waiting for sumo-mcp HTTP endpoint (LuST can take a while to load)"
for _ in $(seq 1 300); do
  code=$(curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:8795/mcp || true)
  [ "$code" != "000" ] && { echo "    endpoint answering (HTTP $code)"; break; }
  sleep 1
done
[ "$code" != "000" ] || { echo "FAIL: sumo-mcp endpoint never answered"; "${COMPOSE[@]}" logs sumo-mcp | tail -40; exit 1; }

echo "==> waiting for the SUMO world to reach the hub"
for _ in $(seq 1 60); do
  # `|| true`: before the first counter log line appears, grep matches nothing
  # and would fail the pipeline under `set -o pipefail` + `set -e`.
  msgs=$("${COMPOSE[@]}" logs hub-spooler 2>/dev/null | grep -oE 'messages=[0-9]+' | tail -1 | cut -d= -f2 || true)
  [ -n "$msgs" ] && [ "$msgs" -gt 0 ] && { echo "    hub ingesting ($msgs messages)"; break; }
  sleep 2
done

echo "==> proving the SUMO world is durable in the hub (QueryEngine)"
Q=$("${COMPOSE[@]}" exec -T hub-spooler hub-query \
      --root /var/lib/veoveo/spool/world \
      --entities '/world/sumo/**' --timeline tick 2>/dev/null || true)
echo "    $Q"
# Each boot streams as sumo-live-<suffix> (one session = one recording), so match
# the recording by its stable prefix rather than an exact id.
REC=$(echo "$Q" | "$PYBIN" -c 'import sys,json;r=json.load(sys.stdin)["rows_by_recording"];k=[x for x in r if x.startswith("sumo-live")];print(k[0] if k else "")')
[ -n "$REC" ] || { echo "FAIL: no sumo-live* recording captured in hub"; "${COMPOSE[@]}" logs sumo-mcp | tail -40; exit 1; }
ROWS=$(echo "$Q" | "$PYBIN" -c "import sys,json;print(json.load(sys.stdin)['rows_by_recording'].get('$REC',0))")
[ "$ROWS" -gt 0 ] || { echo "FAIL: $REC captured 0 rows"; exit 1; }
echo "OK  hub captured $ROWS rows of the live SUMO world ($REC) under /world/sumo/**"

echo "==> driving the served MCP endpoint end to end"
SUMO_MCP_URL="http://127.0.0.1:8795/mcp" "$PYBIN" showcase/sumo/scripts/capstone_client.py

echo "showcase capstone ok"
