#!/usr/bin/env bash
# hub bench: drive a burst sensor stack into the spooler as fast as possible and
# assert the durable capture is lossless — every emitted row is queryable — while
# reporting sustained throughput. Loss is the failure the hub must never have; a
# spooler that can't outrun its producers is a data-loss machine.
#
# Usage: hub_bench.sh [burst_multiplier] [duration_s]
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT"

BURST="${1:-8}"
DURATION="${2:-5}"
PORT="${HUB_PROXY_PORT:-9933}"

SPOOL="$(mktemp -d)"
SP=""
cleanup() { [ -n "$SP" ] && kill "$SP" 2>/dev/null || true; rm -rf "$SPOOL"; }
trap cleanup EXIT

echo "==> building recording-hub binaries (release for the bench)"
cargo build -q --release -p veoveo-recording-hub --bins
SPOOLER=./target/release/spooler
SIM=./target/release/sensor-sim
QUERY=./target/release/hub-query

READY="$SPOOL/ready"
RUST_LOG=warn "$SPOOLER" \
  --bind "127.0.0.1:$PORT" --spool-dir "$SPOOL/spool" \
  --route "world=veoveo-sim" --ready-file "$READY" \
  --flush-interval-ms 250 --segment-max-bytes 1073741824 \
  >"$SPOOL/spooler.log" 2>&1 &
SP=$!
for _ in $(seq 1 100); do [ -f "$READY" ] && break; sleep 0.1; done

echo "==> bursting stack: burst=${BURST}x duration=${DURATION}s (as fast as possible)"
START=$(python3 -c 'import time; print(time.time())')
"$SIM" --proxy "rerun+http://127.0.0.1:$PORT/proxy" \
  --duration-s "$DURATION" --burst "$BURST" --realtime false \
  --report "$SPOOL/report.json" >"$SPOOL/sim.log" 2>&1
END=$(python3 -c 'import time; print(time.time())')
sleep 1
kill -TERM $SP; wait $SP 2>/dev/null || true; SP=

EMITTED=$(python3 -c "import json;print(json.load(open('$SPOOL/report.json'))['total_emitted'])")
"$QUERY" --root "$SPOOL/spool/world" --timeline tick >"$SPOOL/query.json"
CAPTURED=$(python3 -c "import json;print(json.load(open('$SPOOL/query.json'))['total_rows'])")

python3 - "$START" "$END" "$EMITTED" "$CAPTURED" <<'PY'
import sys
start, end, emitted, captured = float(sys.argv[1]), float(sys.argv[2]), int(sys.argv[3]), int(sys.argv[4])
elapsed = max(end - start, 1e-9)
rate = emitted / elapsed
print(f"emitted={emitted} captured={captured} elapsed={elapsed:.2f}s throughput={rate:,.0f} msgs/s")
assert captured == emitted, f"LOSS: captured {captured} != emitted {emitted}"
print("OK  lossless durable capture under burst load")
PY

echo "hub bench ok"
