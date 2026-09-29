# Veoveo MCP Python SDK

`veoveo-mcp` is the supported Python package for an MCP server hosted by a Veoveo
installation. It provides the hosted-server contract, internal
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

`TaskTransition.succeeded(message, payload)` accepts the domain's JSON result.
Snapshots expose it as `TaskResult`, whose `payload` may itself be `None` for JSON
null. `snapshot.result is None` means that the Task has no result. Snapshot JSON omits
an absent result and includes a completed null, following the shared
[Task result format](../../platform/task-runtime/DESIGN.md#result-persistence-and-installation).
The Store adapter preserves nested nulls and unsigned 64-bit integers in results and
event snapshots. The official Tasks adapter returns object results directly and wraps
other payloads in a `value` object.

Public Task handlers use `runtime.for_owner(caller)` to construct an `OwnerTaskQuery`.
Its `get`, `page`, `subscribe`, outstanding-input reads and mutations apply tenant,
principal, profile and label clearance in SQL before decoding Task contents. Indexed
identity must agree with the saved owner. An absent tenant stays distinct from a tenant
named `installation`. `in_work_context()` adds agreement among the indexed context and
both retained authority representations. `of_type(TaskTypeName(...))` and `of_types`
select validated domain operation names without registering them in MCP core.

Queries accept UUIDv7 values parsed at the protocol boundary. Pages use a typed
`TaskPageCursor` containing creation time and Task ID, ordered by both fields, with
1–1000 items selected after authorization. Every page reapplies caller clearance.
Cancellation and input responses recheck the same query inside their mutation
transaction, including after a concurrent policy change. `TaskRuntime.get` and
`live_updates` are trusted worker APIs and supply no caller authorization.

Owner subscriptions admit at most 256 requested Tasks. They select current authorized
rows for notifications, using outbox identities as wake metadata without decoding old
event snapshots. Intermediate states may coalesce. A 15-second current-state check
recovers retained-event gaps. LIVE readers survive idle deadlines and close on
cancellation, acknowledgement failure or notification delivery failure, including
before the first iteration. A terminated source reaches the consumer; a replacement
subscription admits its IDs again and reads a fresh baseline. Call `updates.aclose()`
when consuming the reader outside the SDK's request-scoped Tasks adapter.

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
without editable paths.
