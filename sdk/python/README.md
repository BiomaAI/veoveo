# Veoveo MCP Python SDK

`veoveo-mcp` is the supported Python package for an MCP server hosted by a Veoveo
installation. It provides the hosted-server contract, internal
identity verification, task-extension transport, durable task runtime, artifact
client, schema helpers, pagination, host validation, and the telemetry boundary.

The [server contract design](DESIGN.md) describes nominal scopes and resource types,
owner-defined vocabularies, URI builders and checked MCP setup. New servers implement
`McpServerContract` in their own package and consume `McpServerSetup` before starting
dependencies. Datasheet is the working reference.

Task creation reuses principals by tenant, kind, issuer and subject. Display names
are presentation metadata. Identity discovery creates missing records and checks
existing identities inside one transaction, preserving existing names and security
fields, including a concurrent disablement.

A verified internal identity may carry a `GatewayRequestContext`. It records the source
principal and the signed access-token metadata so they survive delegated calls. The
session family it holds is only an identifier; the context never contains a bearer
token. Before your server grants renewable access, require this context and then check
current policy and grant state yourself. An identity that arrives without the context
cannot be renewed. The verifier rejects an inconsistent context and any assertion that
expires later than its source token. Every supplied context includes a checked
`AuditRequest`: a canonical UUIDv7 request ID, nonzero lowercase trace and span IDs,
and an optional IP address. The verifier preserves managed-agent instance, generation
and epoch metadata. Rust and Python qualify these fields against the same signed-context
fixture; missing audit correlation fails validation.

A simulation server keeps its own world state, camera output, and simulator SDK
integration. It meets the provider-neutral live-view contract through its hosted MCP
server. This package has no API for mirroring scenes or poses into a separate viewer.

Task output capabilities accept `required_data_labels` so that outputs keep the
sensitivity labels of the inputs they came from. The Artifact service adds these labels
to every output and rejects a scope outside the caller's clearance. Work Context policy
still chooses the owner and initial grants.

`TaskTransition.succeeded(message, payload, result_uri=uri_or_none)` accepts the
domain's JSON result with an explicit typed `ResourceUri` or `None`. The address
requires a successful retained result and commits in the same Task transaction.
MCP producers use `mcp_task_completion(message, CallToolResult)` before storing
opaque JSON. It admits one declared nonnull product address and one matching
resource link, including addressable tool errors. It rejects the retired
`result_uri` spelling, including mixed declarations and explicit null values. No-product completions omit the
MCP `resultUri` field and retain `None` in the Task snapshot.
Snapshots expose it as `TaskResult`, whose `payload` may itself be `None` for JSON
null. `snapshot.result is None` means that the Task has no result. Snapshot JSON omits
an absent result and includes a completed null, following the shared
[Task result format](../../platform/task-runtime/DESIGN.md#result-persistence-and-installation).
The Store adapter preserves nested nulls and unsigned 64-bit integers in results and
native changefeed records. The official Tasks adapter returns object results directly and wraps
other payloads in a `value` object.

Public Task handlers use `runtime.for_owner(caller)` to construct an `OwnerTaskQuery`.
Its `get`, `page`, `subscribe`, outstanding-input reads and mutations apply tenant,
principal, profile and label clearance in SQL before decoding Task contents. Indexed
identity must agree with the saved owner. An absent tenant stays distinct from a tenant
named `installation`. `in_work_context()` adds agreement among the indexed context and
both retained authority representations. `of_type(TaskTypeName(...))` and `of_types`
select validated domain operation names without registering them in MCP core.

`new_task_id()` uses `uuid-utils` 1.0.0's standard-library adapter to return a native
`uuid.UUID` with the RFC 9562 48-bit Unix millisecond timestamp. The exact pin selects
the upstream stable release qualified with the Python SDK, template and fork fixture.
SurrealDB receives native UUID values and Pydantic checks the UUIDv7 contract.

Queries accept UUIDv7 values parsed at the protocol boundary. Pages use a typed
`TaskPageCursor` containing creation time and Task ID, ordered by both fields, with
1–1000 items selected after authorization. Every page reapplies caller clearance.
Cancellation and input responses recheck the same query inside their mutation
transaction, including after a concurrent policy change. `TaskRuntime.get` and
`live_updates` are trusted worker APIs and supply no caller authorization.

`query.usage()` preserves the owner query's operation and optional Work Context
selection for domain usage. Its `get` returns typed `UsageRecord` values; `page`
groups authorized rows by their linked Task and returns UUIDs plus a typed continuation.
The usage row's server and tenant must agree with the selected parent. `complete`
filters a Task-ID prefix in SQL before grouping and selecting up to 100 results.

Owner subscriptions admit at most 256 requested Tasks. A projected LIVE query wakes
the reader, which replays native commit identities and selects current authorized rows
in SQL. Intermediate states may coalesce. A cursor older than six days triggers a fresh
baseline on the next wake. Idle readers issue no database queries. Connection loss ends
the reader; a replacement subscription admits its IDs again and reads current state.
Cancellation, acknowledgement failure and notification delivery failure close the owned
LIVE source, including before the first iteration. Call `updates.aclose()` when consuming
the reader outside the SDK's request-scoped Tasks adapter.

Trusted `live_updates_after(TaskUpdateCursor(...))` readers replay committed Task states
using the SurrealDB versionstamp. The returned cursor repeats its complete transaction
on resume, so consumers accept duplicates and persist their acknowledgement after handling
the records. Cursors outside retention start from current Task state. `await_terminal`
reads the requested Task on LIVE changes and observes other replicas without polling.

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
