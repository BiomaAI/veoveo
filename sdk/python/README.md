# Veoveo MCP Python SDK

`veoveo-mcp` is the supported Python package for an independently owned MCP server
hosted by a Veoveo installation. It provides the hosted-server contract, internal
identity verification, task-extension transport, durable task runtime, artifact
client, schema helpers, pagination, host validation, and the telemetry boundary.

A verified internal identity may carry a `GatewayRequestContext`. It records the source
principal and the signed access-token metadata so they survive delegated calls. The
session family it holds is only an identifier; the context never contains a bearer
token. Before your server grants renewable access, require this context and then check
current policy and grant state yourself. An identity that arrives without the context
cannot be renewed. The verifier rejects an inconsistent context and any assertion that
expires later than its source token.

A simulation server keeps its own world state, camera output, and simulator SDK
integration. It meets the provider-neutral live-view contract through its hosted MCP
server. This package has no API for mirroring scenes or poses into a separate viewer.

Task output capabilities accept `required_data_labels` so that outputs keep the
sensitivity labels of the inputs they came from. The Artifact service adds these labels
to every output and rejects a scope outside the caller's clearance. Work Context policy
still chooses the owner and initial grants.

## Development In A Fork

Python hosted servers import this package from the same checkout through a uv path
source. `templates/python-mcp/pyproject.toml` shows the supported layout. Its committed
lockfile pins third-party dependencies; the image build installs both local packages
without editable paths. See [Fork Development](../../docs/FORK_DEVELOPMENT.md).

Run the SDK checks from this directory with `uv run --locked --all-extras pytest`.
Integration tests declare their SurrealDB fixture requirements. Credentials stay in
the installation environment and never enter source files or image layers.

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

`get` and `resolve`, including their `ArtifactRepository` wrappers, load the whole
artifact into memory. They default to an 8 MiB ceiling, accept an explicit `max_bytes`,
and enforce the ceiling while streaming, before allocating. Use `stream` or
`materialize` for large inputs. The size an upload was admitted at does not raise a
domain server's own input limits: Datasheet and pandas keep their own memory and format
constraints. Consumers never see an object-store URL or storage credential.

## Repository Checks

`cargo xtask enforce python` runs the native SDK, template and fork-workload tests
against local source and committed lockfiles. Image builds package these sources
without editable paths. There is no separate Veoveo SDK release-publisher command.
