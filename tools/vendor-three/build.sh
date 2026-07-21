#!/usr/bin/env bash
# Reproducible one-shot build of the self-contained three.js vendor bundle for
# view-mcp. See README.md in this directory for the pin rationale and the
# inlining constraints the output must satisfy.
set -euo pipefail

cd "$(dirname "$0")"

OUT="../../servers/view-mcp/assets/vendor/three-bundle.min.js"
MAX_BYTES=1700000

# Install exactly what package-lock.json pins. The lock file is generated once
# with `npm install` and committed; every later build uses `npm ci`.
if [ -f package-lock.json ]; then
  npm ci --no-audit --no-fund
else
  echo "package-lock.json missing; generating it with npm install (commit the result)"
  npm install --no-audit --no-fund
fi

THREE_VERSION="$(node -p 'JSON.parse(require("fs").readFileSync("node_modules/three/package.json", "utf8")).version')"
echo "three resolved: ${THREE_VERSION}"

mkdir -p "$(dirname "$OUT")"

ESBUILD_OUT="$(mktemp)"
trap 'rm -f "$ESBUILD_OUT"' EXIT

# Part 1: three core plus OrbitControls as an IIFE under the global VENDOR.
./node_modules/.bin/esbuild entry.mjs \
  --bundle \
  --minify \
  --format=iife \
  --global-name=VENDOR \
  --log-level=warning \
  --outfile="$ESBUILD_OUT"

# Part 2: the JS-only Emscripten draco decoder, which defines the global
# DracoDecoderModule. It is a plain script, so plain concatenation is correct.
DRACO="node_modules/three/examples/jsm/libs/draco/gltf/draco_decoder.js"

cat "$ESBUILD_OUT" > "$OUT"
printf '\n' >> "$OUT"
cat "$DRACO" >> "$OUT"

ESBUILD_BYTES="$(wc -c < "$ESBUILD_OUT")"
DRACO_BYTES="$(wc -c < "$DRACO")"

# The artifact is inlined into an HTML <script> tag, so its byte stream must
# not contain a case-insensitive "</script" substring. Escape any occurrence
# as "<\/script" (identical semantics inside JS string and regex literals),
# then fail hard if one survives.
node - "$OUT" <<'NODE'
const fs = require("fs");
const file = process.argv[2];
let src = fs.readFileSync(file, "utf8");
const pattern = /<\/(script)/gi;
const hits = src.match(pattern);
if (hits) {
  src = src.replace(pattern, "<\\/$1");
  fs.writeFileSync(file, src);
  console.log(`escaped ${hits.length} "</script" occurrence(s)`);
}
if (/<\/script/i.test(src)) {
  console.error("FAIL: </script still present after escaping");
  process.exit(1);
}
console.log("ok: no </script substring in artifact");
NODE

# Syntax check: the escaped artifact must still parse as a classic script.
node - "$OUT" <<'NODE'
const fs = require("fs");
new Function(fs.readFileSync(process.argv[2], "utf8"));
console.log("ok: artifact parses (new Function)");
NODE

# Behavior check: evaluating the artifact in a globalThis environment with a
# minimal self/window shim must define VENDOR.THREE, VENDOR.OrbitControls,
# and DracoDecoderModule, and must perform no network call while evaluating.
node - "$OUT" <<'NODE'
const fs = require("fs");
const src = fs.readFileSync(process.argv[2], "utf8");
globalThis.self = globalThis;
globalThis.window = globalThis;
const netCalls = [];
globalThis.fetch = (input) => {
  netCalls.push(String(input));
  throw new Error("network call during evaluation");
};
// XMLHttpRequest and importScripts stay undefined so any evaluation-time use throws.
(0, eval)(src);
const problems = [];
if (typeof globalThis.VENDOR === "undefined") {
  problems.push("VENDOR undefined");
} else {
  if (!globalThis.VENDOR.THREE) problems.push("VENDOR.THREE undefined");
  if (!globalThis.VENDOR.OrbitControls) problems.push("VENDOR.OrbitControls undefined");
}
if (typeof globalThis.DracoDecoderModule !== "function") {
  problems.push("DracoDecoderModule is not a function");
}
if (netCalls.length > 0) {
  problems.push("network calls during evaluation: " + netCalls.join(", "));
}
if (problems.length > 0) {
  console.error("FAIL: " + problems.join("; "));
  process.exit(1);
}
console.log(
  "ok: VENDOR.THREE (r" + globalThis.VENDOR.THREE.REVISION +
  "), VENDOR.OrbitControls, and DracoDecoderModule are defined"
);
NODE

FINAL_BYTES="$(wc -c < "$OUT")"
echo "esbuild part:   ${ESBUILD_BYTES} bytes"
echo "draco part:     ${DRACO_BYTES} bytes"
echo "final artifact: ${FINAL_BYTES} bytes -> ${OUT}"
if [ "${FINAL_BYTES}" -gt "${MAX_BYTES}" ]; then
  echo "FAIL: artifact exceeds ${MAX_BYTES} bytes" >&2
  exit 1
fi
echo "done"
