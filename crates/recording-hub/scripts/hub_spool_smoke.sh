#!/usr/bin/env bash
# hub-spool smoke: process-level proof that the spooler durably captures a
# deterministic stack, survives a kill -9 mid-stream, resumes into a sibling
# segment on restart, and that the local QueryEngine reads back exactly the
# sensor-sim ground truth. No Python or catalog needed — this is the durable
# core.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT"

PORT="${HUB_PROXY_PORT:-9932}"
SPOOL="$(mktemp -d)"
SP=""
cleanup() {
  [ -n "$SP" ] && kill "$SP" 2>/dev/null || true
  rm -rf "$SPOOL"
}
trap cleanup EXIT

echo "==> building recording-hub binaries + query helper"
cargo build -q -p veoveo-recording-hub --bins

start_spooler() {
  READY="$SPOOL/ready.$1"
  RUST_LOG=warn ./target/debug/spooler \
    --bind "127.0.0.1:$PORT" --spool-dir "$SPOOL/spool" \
    --route "world=veoveo-sim" --ready-file "$READY" --flush-interval-ms 100 \
    >"$SPOOL/spooler.$1.log" 2>&1 &
  SP=$!
  for _ in $(seq 1 100); do [ -f "$READY" ] && return 0; sleep 0.1; done
  echo "spooler did not become ready"; exit 1
}

run_sim() {
  ./target/debug/sensor-sim \
    --proxy "rerun+http://127.0.0.1:$PORT/proxy" \
    --duration-s "$1" --realtime false --report "$SPOOL/report.$2.json" \
    >"$SPOOL/sim.$2.log" 2>&1
}

echo "==> session 1: spool, then KILL -9 (no graceful freeze)"
start_spooler s1
run_sim 1.0 s1
sleep 1
kill -9 $SP; wait $SP 2>/dev/null || true; SP=

echo "==> segments survive an ungraceful kill (footer-less, still decodable):"
find "$SPOOL/spool/world" -name '*.rrd' | sort

echo "==> session 2: restart same recordings, must resume into .rN siblings"
start_spooler s2
run_sim 1.0 s2
sleep 1
kill -TERM $SP; wait $SP 2>/dev/null || true; SP=

# The footer-less crash model: a graceful freeze (.r1) writes a footer and
# passes `rerun rrd verify`; a kill -9'd segment has no footer, so `verify`
# (which wants a manifest) rejects it — yet QueryEngine still decodes it
# message-by-message. The QueryEngine cross-check below is the crash-safety
# proof; `verify` here is informational.
echo "==> segment footer status (graceful .rN pass verify; crashed base is footer-less)"
RERUN_BIN="${RERUN_BIN:-$(command -v rerun || true)}"
if [ -n "$RERUN_BIN" ]; then
  while IFS= read -r f; do
    if "$RERUN_BIN" rrd verify "$f" >/dev/null 2>&1; then
      echo "  footered  $(basename "$f")"
    else
      echo "  footerless $(basename "$f")  (crash-safe, QueryEngine-readable)"
    fi
  done < <(find "$SPOOL/spool/world" -name '*.rrd' | sort)
fi

echo "==> QueryEngine read-back cross-check (cumulative across both sessions)"
./target/debug/hub-query --root "$SPOOL/spool/world" --timeline tick >"$SPOOL/query.json"
cat "$SPOOL/query.json"
python3 - "$SPOOL" <<'PY'
import json, sys, glob, os
spool = sys.argv[1]
# Sum expected emissions across both sessions from the sensor-sim reports.
expected = {}
for rep in glob.glob(os.path.join(spool, "report.*.json")):
    d = json.load(open(rep))
    for s in d["sensors"]:
        expected[s["recording"]] = expected.get(s["recording"], 0) + s["emitted"]
# Two physical files per recording (base + .r1) prove resume-not-truncate.
for rec in expected:
    files = glob.glob(os.path.join(spool, "spool", "world", "**", f"{rec}*.rrd"), recursive=True)
    assert len(files) == 2, f"{rec}: expected 2 segments (base + .r1), got {files}"
# QueryEngine row counts equal the cumulative ground truth exactly.
got = json.load(open(os.path.join(spool, "query.json")))["rows_by_recording"]
for rec, n in expected.items():
    assert got.get(rec) == n, f"{rec}: QueryEngine {got.get(rec)} != expected {n}"
print("OK cumulative rows match:", got)
PY

echo "hub-spool smoke ok"
