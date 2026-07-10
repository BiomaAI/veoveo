#!/usr/bin/env bash
set -euo pipefail

bundle=""
runtime=docker
install_dir="${PWD}/veoveo-offline"

usage() {
  printf 'usage: %s --bundle PATH [--runtime docker|containerd] [--install-dir PATH]\n' "$0"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --bundle) bundle="$2"; shift 2 ;;
    --runtime) runtime="$2"; shift 2 ;;
    --install-dir) install_dir="$2"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) usage >&2; exit 2 ;;
  esac
done
[[ -n "$bundle" ]] || { usage >&2; exit 2; }
[[ "$runtime" == docker || "$runtime" == containerd ]] || { usage >&2; exit 2; }
[[ -n "$install_dir" ]] || { printf 'install directory must not be empty\n' >&2; exit 2; }
if [[ -e "$install_dir" ]] && [[ ! -d "$install_dir" ]]; then
  printf 'install path is not a directory: %s\n' "$install_dir" >&2
  exit 1
fi
if [[ -e "$install_dir" ]] && [[ -n "$(find "$install_dir" -mindepth 1 -maxdepth 1 -print -quit 2>/dev/null)" ]]; then
  printf 'install directory is not empty: %s\n' "$install_dir" >&2
  exit 1
fi

for command in jq tar; do
  command -v "$command" >/dev/null || { printf 'missing required command: %s\n' "$command" >&2; exit 1; }
done
if ! command -v sha256sum >/dev/null && ! command -v shasum >/dev/null; then
  printf 'missing required command: sha256sum or shasum\n' >&2
  exit 1
fi

checksum_check() {
  if command -v sha256sum >/dev/null; then sha256sum -c "$1"; else shasum -a 256 -c "$1"; fi
}
if [[ -f "${bundle}.sha256" ]]; then
  (cd "$(dirname "$bundle")" && checksum_check "$(basename "$bundle").sha256")
fi

stage="$(mktemp -d "${TMPDIR:-/tmp}/veoveo-load.XXXXXX")"
cleanup() { rm -rf "$stage"; }
trap cleanup EXIT
tar -C "$stage" -xzf "$bundle"

(cd "$stage" && checksum_check SHA256SUMS)

case "$runtime" in
  docker)
    command -v docker >/dev/null || { printf 'missing required command: docker\n' >&2; exit 1; }
    docker load -i "$stage/images.tar" >/dev/null
    while IFS=$'\t' read -r ref expected; do
      actual="$(docker image inspect "$ref" --format '{{.Id}}')"
      [[ "$actual" == "$expected" ]] || {
        printf 'image identity mismatch for %s: expected %s, got %s\n' "$ref" "$expected" "$actual" >&2
        exit 1
      }
    done < <(jq -r '.images[] | [.ref, .image_id] | @tsv' "$stage/metadata/bundle.json")
    ;;
  containerd)
    command -v ctr >/dev/null || { printf 'missing required command: ctr\n' >&2; exit 1; }
    ctr --namespace k8s.io images import "$stage/images.tar" >/dev/null
    while IFS= read -r ref; do
      ctr --namespace k8s.io images inspect "$ref" >/dev/null 2>&1 \
        || ctr --namespace k8s.io images inspect "docker.io/$ref" >/dev/null
    done < <(jq -r '.images[].ref' "$stage/metadata/bundle.json")
    ;;
esac

mkdir -p "$install_dir"
cp -R "$stage/payload/." "$install_dir/"
mkdir -p "$install_dir/bundle-evidence"
cp -R "$stage/metadata/." "$install_dir/bundle-evidence/"
cp "$stage/SHA256SUMS" "$install_dir/bundle-evidence/SHA256SUMS"

printf 'loaded every verified bundle image and installed configuration at %s\n' "$install_dir"
printf 'Helm offline values: %s/deploy/values.offline.yaml (imagePullPolicy Never)\n' "$install_dir"
