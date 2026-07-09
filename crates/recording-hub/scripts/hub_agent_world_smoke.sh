#!/usr/bin/env bash
# hub-agent-world smoke: two producers, one hub, one unified record. A sensor
# (application id veoveo-sim-*) and an agent-labeled recording (veoveo-agent-*)
# push concurrently; the spooler routes them into different datasets by the
# same rules compose uses (agents=veoveo-agent, world=catch-all), and both are
# queryable side by side. This is the essence of the agent's decision log and the
# sensor world sharing one record — without the full gateway/Cloudflare stack.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT"

PORT="${HUB_PROXY_PORT:-9934}"
STACK="crates/recording-hub/scripts/stacks/agent_world.json"
SPOOL="$(mktemp -d)"
SP=""
cleanup() { [ -n "$SP" ] && kill "$SP" 2>/dev/null || true; rm -rf "$SPOOL"; }
trap cleanup EXIT

echo "==> building recording-hub binaries"
cargo build -q -p veoveo-recording-hub --bins

echo "==> spooler with compose-equivalent routing (agents=veoveo-agent, world=catch-all)"
READY="$SPOOL/ready"
RUST_LOG=warn ./target/debug/spooler \
  --bind "127.0.0.1:$PORT" --spool-dir "$SPOOL/spool" \
  --route "agents=veoveo-agent" --route "world=" \
  --ready-file "$READY" --flush-interval-ms 100 \
  >"$SPOOL/spooler.log" 2>&1 &
SP=$!
for _ in $(seq 1 100); do [ -f "$READY" ] && break; sleep 0.1; done

echo "==> two producers push concurrently (sensor + agent tee)"
./target/debug/sensor-sim \
  --proxy "rerun+http://127.0.0.1:$PORT/proxy" \
  --stack "$STACK" --realtime false --report "$SPOOL/report.json" \
  >"$SPOOL/sim.log" 2>&1
sleep 1
kill -TERM $SP; wait $SP 2>/dev/null || true; SP=

echo "==> segment tree (datasets separated by routing):"
find "$SPOOL/spool" -name '*.rrd' | sort

echo "==> query each dataset and cross-check the unified record"
./target/debug/hub-query --root "$SPOOL/spool/world" --timeline tick >"$SPOOL/world.json"
./target/debug/hub-query --root "$SPOOL/spool/agents" --timeline tick >"$SPOOL/agents.json"
cat "$SPOOL/world.json"; cat "$SPOOL/agents.json"

python3 - "$SPOOL" <<'PY'
import json, os, sys, glob
spool = sys.argv[1]
report = {s["recording"]: s["emitted"] for s in json.load(open(os.path.join(spool, "report.json")))["sensors"]}
world = json.load(open(os.path.join(spool, "world.json")))["rows_by_recording"]
agents = json.load(open(os.path.join(spool, "agents.json")))["rows_by_recording"]

# Routing: the sensor landed in world, the agent recording landed in agents.
assert "sim-gnss-a" in world, f"sensor not in world dataset: {world}"
assert "agent-pilot-episode" in agents, f"agent not in agents dataset: {agents}"
assert "agent-pilot-episode" not in world, "agent leaked into world dataset"
assert "sim-gnss-a" not in agents, "sensor leaked into agents dataset"

# Physical directory separation.
assert glob.glob(os.path.join(spool, "spool", "world", "**", "sim-gnss-a*.rrd"), recursive=True)
assert glob.glob(os.path.join(spool, "spool", "agents", "**", "agent-pilot-episode*.rrd"), recursive=True)

# Counts match the deterministic ground truth exactly, in both datasets.
assert world["sim-gnss-a"] == report["sim-gnss-a"], (world, report)
assert agents["agent-pilot-episode"] == report["agent-pilot-episode"], (agents, report)
print(f"OK  world={world}  agents={agents}  (one hub, two datasets, exact counts)")
PY

echo "hub-agent-world smoke ok"
