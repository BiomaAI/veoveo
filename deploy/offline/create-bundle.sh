#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
lock_file="${repo_root}/deploy/offline/images.lock.json"
output="${repo_root}/output/veoveo-offline-0.1.0.tar.gz"
platform="linux/amd64"
skip_build=false
skip_sbom=false

usage() {
  printf 'usage: %s [--output PATH] [--platform linux/amd64|linux/arm64] [--skip-build] [--skip-sbom]\n' "$0"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --output) output="$2"; shift 2 ;;
    --platform) platform="$2"; shift 2 ;;
    --skip-build) skip_build=true; shift ;;
    --skip-sbom) skip_sbom=true; shift ;;
    -h|--help) usage; exit 0 ;;
    *) usage >&2; exit 2 ;;
  esac
done

for command in cargo docker jq tar; do
  command -v "$command" >/dev/null || { printf 'missing required command: %s\n' "$command" >&2; exit 1; }
done
if ! command -v sha256sum >/dev/null && ! command -v shasum >/dev/null; then
  printf 'missing required command: sha256sum or shasum\n' >&2
  exit 1
fi
case "$platform" in
  linux/amd64|linux/arm64) ;;
  *) printf 'unsupported platform: %s\n' "$platform" >&2; exit 2 ;;
esac
if [[ "$skip_sbom" != true ]]; then
  command -v syft >/dev/null || {
    printf 'syft is required for SBOM generation; use --skip-sbom only for an explicitly non-release bundle\n' >&2
    exit 1
  }
fi

jq -e --slurp --arg profile lock -f "$repo_root/deploy/offline/admission.jq" "$lock_file" >/dev/null || {
  printf 'invalid or unsupported image manifest\n' >&2
  exit 1
}
while IFS= read -r dockerfile; do
  [[ -f "$repo_root/$dockerfile" ]] || {
    printf 'missing Dockerfile from image manifest: %s\n' "$dockerfile" >&2
    exit 1
  }
done < <(jq -r '.veoveoImages[].dockerfile' "$lock_file")

stage="$(mktemp -d "${TMPDIR:-/tmp}/veoveo-offline.XXXXXX")"
cleanup() { rm -rf "$stage"; }
trap cleanup EXIT
mkdir -p "$stage/payload/configs" "$stage/payload/deploy/offline" "$stage/payload/schemas" "$stage/metadata/sbom"

cargo run --manifest-path "$repo_root/Cargo.toml" \
  -p veoveo-gateway-composition --no-default-features --features smoke --bin gateway-smoke-support -- \
  contract-schemas --output-dir "$stage/payload/schemas"

while IFS=$'\t' read -r ref source; do
  docker pull --platform "$platform" "$source"
  docker tag "$source" "$ref"
done < <(jq -r '.externalImages[] | [.ref, .source] | @tsv' "$lock_file")

while IFS=$'\t' read -r ref dockerfile; do
  if [[ "$skip_build" != true ]]; then
    docker build --platform "$platform" --pull -f "$repo_root/$dockerfile" -t "$ref" "$repo_root"
  fi
  docker image inspect "$ref" >/dev/null
done < <(jq -r '.veoveoImages[] | [.ref, .dockerfile] | @tsv' "$lock_file")

mapfile -t image_refs < <(jq -r '.externalImages[].ref, .veoveoImages[].ref' "$lock_file")
docker save -o "$stage/images.tar" "${image_refs[@]}"

jq -n --arg platform "$platform" --slurpfile lock "$lock_file" \
  '{schemaVersion: 2, platform: $platform, bundle: $lock[0], images: []}' >"$stage/metadata/bundle.json"
for ref in "${image_refs[@]}"; do
  image_id="$(docker image inspect "$ref" --format '{{.Id}}')"
  jq --arg ref "$ref" --arg image_id "$image_id" \
    '.images += [{ref: $ref, imageId: $image_id}]' \
    "$stage/metadata/bundle.json" >"$stage/metadata/bundle.json.next"
  mv "$stage/metadata/bundle.json.next" "$stage/metadata/bundle.json"
  if [[ "$skip_sbom" != true ]]; then
    sbom_name="$(printf '%s' "$ref" | tr '/:@' '____').spdx.json"
    syft "$ref" -o "spdx-json=$stage/metadata/sbom/$sbom_name"
  fi
done

jq -e --slurp --arg profile bundle -f "$repo_root/deploy/offline/admission.jq" "$stage/metadata/bundle.json" >/dev/null || {
  printf 'invalid or incomplete bundle image identities\n' >&2
  exit 1
}

cp "$repo_root/configs/deployments.json" "$stage/payload/configs/deployments.json"
cp "$repo_root/configs/gateway.local.json" "$stage/payload/configs/gateway.local.json"
cp "$repo_root/configs/otel-collector.yaml" "$stage/payload/configs/otel-collector.yaml"
cp "$repo_root/configs/otel-collector.siem.example.yaml" "$stage/payload/configs/otel-collector.siem.example.yaml"
cp -R "$repo_root/configs/stream" "$stage/payload/configs/stream"
cp "$repo_root/.env.example" "$stage/payload/.env.example"
cp -R "$repo_root/assets" "$stage/payload/assets"
cp -R "$repo_root/deploy/helm" "$stage/payload/deploy/helm"
cp "$repo_root/deploy/offline/values.offline.yaml" "$stage/payload/deploy/values.offline.yaml"
cp "$repo_root/deploy/offline/load-bundle.sh" "$stage/payload/deploy/offline/load-bundle.sh"
cp "$repo_root/deploy/offline/admission.jq" "$stage/payload/deploy/offline/admission.jq"
cp "$repo_root/deploy/offline/README.md" "$stage/payload/deploy/offline/README.md"
cp "$lock_file" "$stage/metadata/images.lock.json"

checksum() {
  if command -v sha256sum >/dev/null; then sha256sum "$@"; else shasum -a 256 "$@"; fi
}
(
  cd "$stage"
  while IFS= read -r file; do checksum "$file"; done \
    < <(find . -type f ! -name SHA256SUMS | LC_ALL=C sort) >SHA256SUMS
)

mkdir -p "$(dirname "$output")"
tar -C "$stage" -czf "$output" .
(cd "$(dirname "$output")" && checksum "$(basename "$output")" >"$(basename "$output").sha256")
printf 'created %s\n' "$output"
