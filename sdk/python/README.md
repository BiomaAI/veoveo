# Veoveo MCP Python SDK

`veoveo-mcp` is the supported Python package for an independently owned MCP server
hosted by a Veoveo installation. It provides the hosted-server contract, internal
identity verification, task-extension transport, durable task runtime, artifact
client, schema helpers, pagination, host validation, and the telemetry boundary.

Verified internal identities may carry `GatewayRequestContext`, which preserves the
source principal and signed access-token metadata across delegated calls. Its session
family is an identifier, and the context contains no bearer token. A consumer must
require this context before admitting renewable access, then check current policy and
grant state itself. The verifier rejects inconsistent context and an assertion that
outlives its source token. Context omission does not supply renewal authority.

Simulation implementations own their authoritative world, camera products, and
simulation-specific SDK integration. They conform to the provider-neutral live-view
contract through their hosted MCP server rather than publishing a visualization-only
scene or pose mirror through this package.

Task output capabilities accept `required_data_labels` to preserve sensitivity
inherited from domain inputs. The Artifact service adds these labels to every
output and rejects a scope outside the caller's clearance. Work Context policy
continues to select the owner and initial grants.

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

## Streaming Artifact Consumption

`HttpArtifactPlane.stream` consumes a canonical `artifact://{uuidv7}` URI under the
forwarded caller identity. Every consumer declares its own byte ceiling. Iteration
uses bounded chunks and closes the HTTP response when the context exits, including
early exit and task cancellation. Consume the full iterator to check exact length;
pass the upload receipt's `expected_sha256` to verify its whole-file digest as well.

```python
from veoveo_mcp.artifacts import HttpArtifactPlane

plane = HttpArtifactPlane(artifact_service_url)
try:
    async with plane.stream(caller, artifact_uri, max_bytes=20 * 1024**3) as download:
        async for chunk in download:
            await consume_chunk(chunk)
finally:
    await plane.close()
```

For libraries that consume paths, `materialize` yields a fully downloaded temporary
file and removes it on context exit. The filename preserves its extension. Partial
files are removed after transport errors, cancellation, or failed digest validation.

```python
async with plane.materialize(caller, artifact_uri, max_bytes=20 * 1024**3) as path:
    await consume_file(path)
```

`get` and `resolve`, including their `ArtifactRepository` wrappers, accept an explicit
`max_bytes` consumer ceiling. They remain in-memory convenience operations with an 8 MiB default
consumer ceiling and streaming enforcement before allocation. Large inputs use
`stream` or `materialize`. An upload's admitted size does not change a domain server's
own input limits: Datasheet and pandas still impose their separate memory and format
constraints. No object-store URL or storage credential reaches a consumer.

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
