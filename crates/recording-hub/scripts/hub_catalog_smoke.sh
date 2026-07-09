#!/usr/bin/env bash
# hub-catalog smoke: spool a deterministic stack, freeze+optimize the segments,
# serve them from the Rerun catalog, and cross-check a real over-the-wire redap
# query against the sensor-sim ground truth. Segment id must equal recording id
# and per-recording row counts must match exactly.
#
# Requires: the `rerun` CLI on PATH, and a Python env with
# `rerun-sdk[datafusion]==0.34.*` (see hub_python_env.sh).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT"

RERUN_BIN="${RERUN_BIN:-$(command -v rerun)}"
PYTHON_BIN="${HUB_PYTHON:-python3}"
PORT="${HUB_PROXY_PORT:-9931}"
CATPORT="${HUB_CATALOG_PORT:-51331}"

SPOOL="$(mktemp -d)"
SP=""
CAT=""
cleanup() {
  [ -n "$SP" ] && kill "$SP" 2>/dev/null || true
  [ -n "$CAT" ] && kill -9 "$CAT" 2>/dev/null || true
  rm -rf "$SPOOL"
}
trap cleanup EXIT

echo "==> building recording-hub binaries"
cargo build -q -p veoveo-recording-hub --bins

echo "==> spooling a deterministic stack (freeze optimizes via rerun CLI)"
READY="$SPOOL/ready"
RUST_LOG=warn ./target/debug/spooler \
  --bind "127.0.0.1:$PORT" --spool-dir "$SPOOL/spool" \
  --route "world=veoveo-sim" --ready-file "$READY" \
  --rerun-bin "$RERUN_BIN" --flush-interval-ms 100 \
  >"$SPOOL/spooler.log" 2>&1 &
SP=$!
for _ in $(seq 1 100); do [ -f "$READY" ] && break; sleep 0.1; done

./target/debug/sensor-sim \
  --proxy "rerun+http://127.0.0.1:$PORT/proxy" \
  --duration-s 1.0 --realtime false --report "$SPOOL/report.json" \
  >"$SPOOL/sim.log" 2>&1
sleep 1
kill -TERM $SP; wait $SP 2>/dev/null || true
SP=

echo "==> frozen + optimized segments:"
find "$SPOOL/spool/world" -name '*.rrd' | sort

echo "==> launching catalog"
"$RERUN_BIN" server -d "world=$SPOOL/spool/world" --port "$CATPORT" --host 127.0.0.1 \
  >"$SPOOL/catalog.log" 2>&1 &
CAT=$!
for _ in $(seq 1 100); do nc -z 127.0.0.1 "$CATPORT" 2>/dev/null && break; sleep 0.1; done
sleep 1

echo "==> real redap query"
"$PYTHON_BIN" crates/recording-hub/scripts/catalog_query.py \
  "rerun+http://127.0.0.1:$CATPORT" world "$SPOOL/spool/world" tick \
  >"$SPOOL/query.json" 2>"$SPOOL/query.err" || { cat "$SPOOL/query.err"; exit 1; }
kill -9 $CAT 2>/dev/null || true
CAT=

echo "==> cross-check catalog vs sensor-sim ground truth"
"$PYTHON_BIN" - "$SPOOL/query.json" "$SPOOL/report.json" <<'PY'
import json, sys
query = json.load(open(sys.argv[1]))
report = json.load(open(sys.argv[2]))

expected = {s["recording"]: s["emitted"] for s in report["sensors"]}
got = query["rows_by_segment"]

# segment ids equal recording ids
assert sorted(query["segment_ids"]) == sorted(expected), \
    f"segment ids {query['segment_ids']} != recordings {sorted(expected)}"
# per-recording row counts match exactly
for rec, n in expected.items():
    assert got.get(rec) == n, f"{rec}: catalog {got.get(rec)} != expected {n}"
assert query["total_rows"] == report["total_emitted"], \
    f"total {query['total_rows']} != {report['total_emitted']}"
print(f"OK  segments={query['segment_ids']}  rows={got}  total={query['total_rows']}")
PY

echo "hub-catalog smoke ok"
