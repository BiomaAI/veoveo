# Anonymous Simulation MCP Server — Agent Manual

## Purpose

Publish fixture-owned declarative scene assets and complete synthetic
latest-pose snapshots for independent Simulation View acceptance.

## Invariants

- Camera, render-product, RTX, NVENC, WebRTC, signaling, lease, and live-App
  behavior belongs to Simulation View.
- This extension does not implement vehicle dynamics, controls, domain
  sensors, or provider runtime integration.
- Python dependencies resolve from the selected compatibility release through
  the repository-native lock. The source tree never imports a Veoveo checkout.
- Assets remain immutable, content addressed, bounded, declarative, and
  non-executable.
- Pose publication uses the installation-authorized producer ID and exact
  SPIFFE client identity over mutual TLS.
- Credentials and access tokens never enter source, resources, logs, release
  manifests, or image layers.
- The MCP endpoint and `/anonymous-simulation/admin/docs/*` projection pass
  through the same gateway internal-auth middleware.

## Build And Test

- `uv sync --locked --all-extras`
- `uv run --locked --all-extras pytest`
- `uv build`
- `helm lint deploy/helm`
- `docker buildx bake anonymous-simulation-extension --print`

Runtime live-view acceptance belongs to Simulation View and requires an
accessible NVIDIA GPU plus a headed hardware-backed browser.

## Contract Compliance

Contract revision: 2

- C01: met
- C02: met
- C03: met
- C04: met
- C05: met
- C06: met
- C07: met
- C08: met
- C09: met
- C10: met
- C11: met
- C12: met
- C13: met
- C14: met
- C15: met
- C16: met
- C17: met
- C18: met
- C19: met
- C20: met
- C21: met
- C22: met
- C23: met
- C24: met
- C25: met
- C26: met
- C27: met
- C28: met
- C29: met
- C30: met
