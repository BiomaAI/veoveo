# Anonymous External Simulation Extension

This fixture represents a simulation MCP server built in an independent repository.
It owns one simulator-hosted logical camera, bounded viewer-product slots, ephemeral viewer leases,
signaling authorization, and an MCP App. It imports no platform renderer or source
checkout.

The compatibility release selects `veoveo-mcp==0.1.0` for CPython 3.13. The committed
`uv.lock` records the private index coordinate and exact wheel hash. Credentials enter
only through uv's named-index environment variables:

```sh
export UV_INDEX_VEOVEO_USERNAME=token
export UV_INDEX_VEOVEO_PASSWORD="$PACKAGE_TOKEN"
uv sync --locked --all-extras
uv run --locked --all-extras pytest
uv build
```

The Docker build receives the package credential through the
`veoveo-python-index` BuildKit secret. The checkout never refers to a Veoveo filesystem
path and does not join its Cargo workspace.

The extension owns its release manifest, gateway fragment, hosted-server conformance
document, Helm chart, and application image. The adjacent installation fixture owns the
gateway binding, trust, public endpoints, platform selection, and digest lock.

Canonical acceptance copies this directory into a temporary independent checkout,
serves the SDK wheel from an authenticated package index, and runs locked tests,
packaging, Bake graph validation, and Helm validation there. Its synthetic product
cannot count as GPU, NVENC, WebRTC media, or browser visual evidence.
