# Veoveo MCP Python SDK

`veoveo-mcp` is the supported Python package for an independently owned MCP server
hosted by a Veoveo installation. It provides the hosted-server contract, internal
identity verification, task-extension transport, durable task runtime, artifact
client, schema helpers, pagination, host validation, and the telemetry boundary.

Simulation implementations own their authoritative world, camera products, and
simulation-specific SDK integration. They conform to the provider-neutral live-view
contract through their hosted MCP server rather than publishing a visualization-only
scene or pose mirror through this package.

## Supported release

The package is distributed as an immutable wheel and source distribution through a
configured private Python package index. The compatibility manifest names the exact
package version, SHA-256 digests, supported Python range, and contract revisions.

An extension repository pins the supported version:

```toml
[project]
dependencies = ["veoveo-mcp==0.1.0"]
```

The installation operator provides an authenticated PEP 503-compatible index. With
uv, configure the index URL outside source control and then generate the extension's
own lock:

```sh
export UV_DEFAULT_INDEX=https://packages.example.internal/simple
uv lock
uv sync --locked
```

Credentials belong in the package manager's credential provider or its documented
environment variables. They do not belong in `pyproject.toml`, a lockfile, a
Dockerfile, or an extension release manifest.

## Development

The Veoveo repository tests the source workspace and then rebuilds the template in an
isolated directory against the produced wheel:

```sh
cargo xtask enforce python
```

Release artifacts come from an exact committed revision:

```sh
cargo xtask release python-sdk \
  --revision <commit> \
  --output-dir output/releases/python-sdk
```

Add `--publish-url` and optionally `--check-url` to upload to a private index. The
command accepts credentials only through `UV_PUBLISH_*` or the configured keyring.
