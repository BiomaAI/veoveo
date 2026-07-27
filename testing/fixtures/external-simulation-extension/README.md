# Anonymous External Simulation Extension

This fixture represents a repository owned outside Veoveo. It publishes two
declarative OpenUSD assets and complete moving-entity snapshots through the
public Simulation View scene contract and private mTLS pose protocol.

Simulation View owns scene mirroring, camera admission, RTX render products,
NVENC, WebRTC, leases, signaling, and the generic MCP App. This extension owns
none of those mechanisms. It needs no GPU because it is only a synthetic pose
producer; the installation schedules the independent
`simulation-view-renderer` on NVIDIA hardware.

## Published Inputs

The compatibility release selects `veoveo-mcp==0.1.0` for CPython 3.13. The
repository-native `uv.lock` records the private index coordinate and exact
wheel hash. Credentials are provided only through uv's named-index
environment variables:

```sh
export UV_INDEX_VEOVEO_USERNAME=token
export UV_INDEX_VEOVEO_PASSWORD="$PACKAGE_TOKEN"
uv sync --locked --all-extras
uv run --locked --all-extras pytest
uv build
```

The Docker build receives the private index token through the
`veoveo-python-index` BuildKit secret. The named index URL stays in
`pyproject.toml`, while credentials remain outside source control. The source
checkout never refers to a Veoveo filesystem path and does not join its Cargo
workspace.

## Release Surface

The extension owns:

- `release/extension-release.json`;
- `gateway-fragment.json`;
- `conformance/hosted-server.json`;
- `deploy/helm`, including `Chart.lock`;
- one CPU-only application image built by `docker-bake.hcl`.

The adjacent installation repository fixture owns `gateway-binding.json`,
platform selection, trust roots, the producer certificate, public network
coordinates, GPU placement, and image digests.

The canonical acceptance copies this fixture into a temporary independent
Git checkout, serves the SDK wheel from an authenticated package index, and
runs locked tests and packaging there. No Python import resolves from the
Veoveo source tree.
