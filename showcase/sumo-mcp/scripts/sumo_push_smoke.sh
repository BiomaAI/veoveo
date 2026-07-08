#!/usr/bin/env bash
# S0 push spine smoke: the SUMO sim (fake driver) pushes typed world state into
# the real Recording Hub spooler, and the hub's QueryEngine reads the SUMO
# recording back. Proves SUMO is just another hub producer — no pull.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT"

PORT="${HUB_PROXY_PORT:-9935}"
STEPS="${SUMO_STEPS:-40}"
VENV="${SUMO_MCP_VENV:-/tmp/veoveo-sumo-venv}"
SPOOL="$(mktemp -d)"
SP=""
cleanup() { [ -n "$SP" ] && kill "$SP" 2>/dev/null || true; rm -rf "$SPOOL"; }
trap cleanup EXIT

echo "==> ensure python env with rerun SDK"
if [ ! -x "$VENV/bin/python3" ]; then uv venv "$VENV" --python 3.11 >&2; fi
uv pip install --python "$VENV/bin/python3" -e "showcase/sumo-mcp[rerun,dev]" >/tmp/sumo_push_env.log 2>&1 \
  || { echo "env install failed"; tail -15 /tmp/sumo_push_env.log; exit 1; }

echo "==> build hub spooler + query"
cargo build -q -p veoveo-recording-hub --bins

echo "==> start hub spooler (route world=veoveo-sumo)"
READY="$SPOOL/ready"
RUST_LOG=warn ./target/debug/spooler \
  --bind "127.0.0.1:$PORT" --spool-dir "$SPOOL/spool" \
  --route "world=veoveo-sumo" --ready-file "$READY" --flush-interval-ms 100 \
  >"$SPOOL/spooler.log" 2>&1 &
SP=$!
for _ in $(seq 1 100); do [ -f "$READY" ] && break; sleep 0.1; done

echo "==> SUMO sim pushes ${STEPS} frames into the hub"
"$VENV/bin/sumo-sim" \
  --proxy "rerun+http://127.0.0.1:$PORT/proxy" \
  --recording "sumo-run" --steps "$STEPS" --vehicles 6 --seed 3 \
  >"$SPOOL/push.log" 2>&1 || { echo "push failed"; cat "$SPOOL/push.log"; exit 1; }
cat "$SPOOL/push.log"
sleep 1
kill -TERM $SP; wait $SP 2>/dev/null || true; SP=

echo "==> segment tree:"
find "$SPOOL/spool" -name '*.rrd' | sort

echo "==> QueryEngine read-back of the SUMO recording"
./target/debug/hub-query --root "$SPOOL/spool/world" --timeline tick --entities "/world/sumo/**" >"$SPOOL/q.json"
cat "$SPOOL/q.json"
python3 - "$SPOOL" "$STEPS" <<'PY'
import json, sys, glob, os
spool, steps = sys.argv[1], int(sys.argv[2])
q = json.load(open(os.path.join(spool, "q.json")))
rows = q["rows_by_recording"]
assert "sumo-run" in rows, f"SUMO recording not captured: {rows}"
# One dataframe row per tick (columns = the world/sumo entities at that tick).
assert rows["sumo-run"] == steps, f"expected {steps} tick rows, got {rows['sumo-run']}"
assert glob.glob(os.path.join(spool, "spool", "world", "**", "sumo-run*.rrd"), recursive=True), \
    "SUMO segment not routed into world dataset"
print(f"OK  SUMO pushed {rows['sumo-run']} frames, durable in the hub world dataset")
PY

echo "sumo push smoke ok"
